# script-networking Specification

## Purpose
Game scripts talk to services over the network: the web globals a client such as `@abyme/client`'s `connect()` needs (`WebSocket` over `ws:`, timers on real time, `URL`, `AbortController`, `process.env`), deny by default through `net.allow` and `env`, with connections owned by the engine, scoped to a generation and to play, bounded, and testable without a network.

## Requirements

### Requirement: A WebSocket client with the web's shape
`new WebSocket(url, protocols?)` SHALL accept a `ws:` or `wss:` URL (`http:` is read as `ws:` and `https:` as `wss:`, as the web does) and, as the web does, a protocol string or an array of them. As a marked, Deno-shaped extension, the second argument MAY instead be `{ protocols, headers }`; nothing in bevy-ts or its examples SHALL depend on `headers`, and the types SHALL document it as non-standard. The object SHALL have `url`, `protocol`, `extensions`, `readyState` (`CONNECTING` 0, `OPEN` 1, `CLOSING` 2, `CLOSED` 3, as instance and static constants), `bufferedAmount` and `binaryType`; `send(data)` with a string, `ArrayBuffer`, typed array view or `Blob`; `close(code?, reason?)`; events `open`, `message` (`MessageEvent` with `data`), `error` (a plain `Event`) and `close` (`CloseEvent` with `code`, `reason`, `wasClean`), through both `onX` properties and `addEventListener`. Text frames SHALL arrive as strings. Binary frames SHALL arrive as `ArrayBuffer` when `binaryType` is `"arraybuffer"` and as a `Blob` with an empty `type` when it is `"blob"`. `binaryType` SHALL default to `"arraybuffer"`, which differs from the web's `"blob"` and keeps stage 1's scripts working; any other value SHALL be ignored, as the web does. A `Blob` sent SHALL go as one binary frame, in order with the other sends, and count in `bufferedAmount`. `close` SHALL throw `InvalidAccessError` for a code other than 1000 or 3000-4999 and `SyntaxError` for a reason over 123 UTF-8 bytes. A dropped connection SHALL report code 1006 with `wasClean` false; a close frame without a code SHALL report 1005. `send` while `CONNECTING` SHALL throw `InvalidStateError`; `send` after close SHALL be ignored, as the web does.

#### Scenario: Echo
- **WHEN** a script opens `ws://127.0.0.1:9001/echo` (allowed) to a server that echoes, and on `open` sends `"hi"` and a 3-byte `Uint8Array`
- **THEN** `message` fires twice, first with `data === "hi"`, then with an `ArrayBuffer` of those 3 bytes

#### Scenario: Binary as a Blob
- **WHEN** a script sets `binaryType = "blob"` on an open echo socket and sends `new Blob(["ab"])`
- **THEN** `message` fires with a `Blob` of size 2 whose `text()` resolves to `"ab"`

#### Scenario: A bad close code
- **WHEN** a script calls `ws.close(1001)`
- **THEN** it throws a `DOMException` named `InvalidAccessError` and the socket stays open

#### Scenario: The server goes away
- **WHEN** the server drops the TCP connection without a close frame
- **THEN** `error` fires, then `close` with `code` 1006 and `wasClean` false, and `readyState` is 3

### Requirement: The handshake carries no Origin
The opening handshake SHALL send no `Origin` header, as Deno and other native runtimes do: a script has no page origin, and a server such as abymed answers `403` to a foreign one. A `headers` option naming `Origin` SHALL be refused as the non-loopback case is, the log saying the engine sends no `Origin`.

#### Scenario: Opening a connection
- **WHEN** a script opens an allowed `ws://127.0.0.1:9001/` and the test server records the handshake request
- **THEN** the request has no `Origin` header

#### Scenario: A script tries to set one
- **WHEN** a script opens an allowed loopback URL with `{ headers: { Origin: "http://evil" } }`
- **THEN** no connection is attempted, `close` fires with 1006, and the log says the engine sends no `Origin`

### Requirement: Connections only to allowed hosts
The engine SHALL check every `WebSocket` URL, every `fetch` URL and every redirect hop against the allowlist before any DNS lookup or socket. An allowlist entry SHALL be `host` (any port), `host:port`, or a URL whose host and port are taken (a URL without a port takes its scheme's: 80 for `http:`/`ws:`, 443 for `https:`/`wss:`); the scheme of an entry SHALL NOT limit the schemes allowed to that host and port. Host names compare case-insensitively and literally (`localhost` does not allow `127.0.0.1`). A refused `WebSocket` SHALL open no socket: the constructor SHALL succeed, then `error` and `close` (1006) SHALL fire on the next delivery. A refused `fetch` SHALL open no socket and its promise SHALL reject with a `TypeError` on the next delivery. In both cases the log SHALL name the origin and say to add it to `net.allow` in `bevy-ts.json`. A `wss:` URL to an allowed host SHALL open over TLS, verifying the server's certificate against the webpki root store and the URL's host name; a failed verification SHALL fail the open as a dropped connection does (`error`, then `close` with 1006), the log naming the TLS error. A `WebSocket` URL of `https:` SHALL be read as `wss:`, as `http:` is read as `ws:`. A `WebSocket` with a `headers` option SHALL be refused the same way as a host not allowed unless its host is loopback (`localhost`, `127.0.0.0/8`, `::1`), or its scheme is `wss:` and its host and port are the origin the embedding host named in `headersOrigin` (engine-embedding). The log SHALL say, for a `ws:` URL, that headers are sent in clear only over loopback and to use `wss:`; for a `wss:` URL, that headers are sent only to loopback or, over `wss:`, the host's named origin. An engine with no named origin (the CLI, an embedding without `headersOrigin`) SHALL send `WebSocket` headers to loopback, or over `wss:` to any allowed host. A `fetch`'s own headers SHALL follow fetch's rules, unchanged by `headersOrigin`: the device key never reaches a script's headers, as the host passes it as `originHeaders`, which the engine alone adds (engine-embedding). An empty allowlist SHALL refuse everything.

#### Scenario: Not allowed
- **WHEN** `net.allow` is `["127.0.0.1:4477"]` and a script opens `ws://example.com/`
- **THEN** no connection is attempted, `close` fires with 1006, and the log says `example.com` is not in `net.allow`

#### Scenario: A key to another machine
- **WHEN** `net.allow` has `10.0.0.5:4477` and a script opens `ws://10.0.0.5:4477/` with `{ headers: { authorization: "Bearer k" } }`
- **THEN** no connection is attempted, `close` fires with 1006, and the log says headers are sent in clear only over loopback

#### Scenario: TLS
- **WHEN** `net.allow` has `mac.tail.ts.net:8443`, the embedding host named `headersOrigin` `https://mac.tail.ts.net:8443`, and a script opens `wss://mac.tail.ts.net:8443/api/stream` with `{ headers: { authorization: "Bearer k" } }`
- **THEN** the connection opens over TLS and the handshake request carries the `authorization` header

#### Scenario: A key to another allowed TLS host
- **WHEN** `net.allow` has `mac.tail.ts.net:8443` and `other.example.com`, `headersOrigin` is `https://mac.tail.ts.net:8443`, and a script opens `wss://other.example.com/` with headers
- **THEN** no connection is attempted, `close` fires with 1006, and the log says headers are sent only to loopback or, over `wss:`, the host's named origin

#### Scenario: A certificate that does not verify
- **WHEN** a script opens an allowed `wss:` URL whose server presents a self-signed certificate
- **THEN** `error` fires, then `close` with 1006, and the log names the TLS error

#### Scenario: A fetch not allowed
- **WHEN** `net.allow` is `["127.0.0.1:4477"]` and a script calls `fetch("https://example.com/")`
- **THEN** no connection is attempted, the promise rejects with a `TypeError`, and the log says `example.com:443` is not in `net.allow`

### Requirement: Network and environment settings in the project file
`bevy-ts.json` SHALL accept `net.allow`, an array of allowlist entries, and `env`, an array of environment variable names. An entry of the form `${NAME}` SHALL be read from the environment when the engine starts; an unset or empty variable SHALL add nothing. `process.env` SHALL show exactly the names in `env` that are set, read when the engine starts, and nothing else; it SHALL be the same frozen-in-content object for the whole run. Both default to empty. `bevy-ts dev` SHALL read them when it starts the engine; a change to them SHALL apply when the engine next starts. `bevy-ts build` SHALL write both, unexpanded, into `game/game.json` as `net` and `env`, and a shipped game SHALL read them from there, expanding `${NAME}` from its own environment at start. An engine started by an embedding host SHALL take the environment from the host's `env` map instead of the process environment, for both `${NAME}` and `process.env`. A `game.json` without them SHALL mean both empty.

#### Scenario: The abyme client finds abymed
- **WHEN** `bevy-ts.json` has `"net": { "allow": ["127.0.0.1:4477", "${ABYME_URL}"] }` and `"env": ["ABYME_URL", "ABYME_KEY"]`, and the engine starts with `ABYME_URL=http://127.0.0.1:5000` and `HOME` set
- **THEN** a script may connect to `127.0.0.1:4477` and `127.0.0.1:5000`, `process.env.ABYME_URL` is `"http://127.0.0.1:5000"`, `process.env.ABYME_KEY` is `undefined`, and `process.env.HOME` is `undefined`

#### Scenario: A built game
- **WHEN** that project is built and the built game starts with `ABYME_URL` unset
- **THEN** `game/game.json` holds `"${ABYME_URL}"` as written, and the game may connect only to `127.0.0.1:4477`

#### Scenario: An embedded game
- **WHEN** that built game is created by an embedding host with `env: { "ABYME_URL": "https://mac.tail.ts.net:8443" }` while the process environment has `ABYME_URL=http://127.0.0.1:5000`
- **THEN** a script may connect to `mac.tail.ts.net:8443` and not to `127.0.0.1:5000`, and `process.env.ABYME_URL` is the host's value

### Requirement: Events and timers reach JS once a frame
Socket I/O SHALL run off the main thread. The engine SHALL deliver, in `First`, after time advances and after a pending generation is applied and before the backend is pumped, every socket event received since the last delivery, then every timer that is due, in one call into JS; it SHALL make no call into JS when no event is pending and no timer is due. Requests a script makes (open, send, close) SHALL reach the I/O side by the next delivery. Delivery SHALL run every frame in play mode, paused or not, and never in edit mode. Within one delivery, a connection's events SHALL keep their order, and microtasks queued by callbacks SHALL run before the delivery returns. A callback SHALL run outside any system: a host call (a Rust param) made from it SHALL throw the existing "only while a system runs" error, and its effect on the world waits for a system to read the state it set.

#### Scenario: Many messages, one call
- **WHEN** a server sends 500 small messages between two frames
- **THEN** the next frame delivers all 500 in order in one call into JS

#### Scenario: Idle
- **WHEN** no socket has an event and no timer is due for 100 frames
- **THEN** the backend's count of calls into JS does not go up for delivery in those frames

### Requirement: Timers run on real time
Timers SHALL measure `Time<Real>`, so they run while the game is paused or stepped. In edit mode no timer fires; a timer due then fires at the first delivery after the game enters play. A timer SHALL fire at the first delivery at or after its deadline; one frame is the granularity. Due timers SHALL fire in deadline order, ties in creation order. An interval delayed by a long frame SHALL fire once and reschedule from the time it fired, with no catch-up. `setTimeout` and `setInterval` SHALL return a positive integer id, accept extra arguments passed to the callback, treat a missing or negative delay as 0, and accept a string callback as an error. `clearTimeout` and `clearInterval` SHALL accept any id, including one already fired.

#### Scenario: Backoff while paused
- **WHEN** the game is paused and a script set `setTimeout(f, 1000)`
- **THEN** `f` runs about one second of real time later, while still paused

#### Scenario: Deterministic in tests
- **WHEN** a headless test advances `Time<Real>` by 16 ms a frame with `TimeUpdateStrategy::ManualDuration` and a script set `setTimeout(f, 50)`
- **THEN** `f` runs in the fourth frame after it was set, never earlier

### Requirement: Connections and timers belong to a generation
Each connection, fetch and timer SHALL belong to the generation that made it: the loading one while a module evaluates, else the one whose code made the call (a system, an entry, or a callback of a connection, fetch or timer of that generation). When a generation is committed, the engine SHALL close the previous generation's open connections with 1001 ("going away"), cancel its fetches in flight and drop its timers, delivering no event to them and settling none of their promises. When a load fails or is discarded, the engine SHALL close the connections, cancel the fetches and drop the timers that load made, and touch nothing of the live generation. A `restart` reload or an engine instance replaced for heap size SHALL close every connection, cancel every fetch and drop every timer. A failed compile SHALL touch nothing.

#### Scenario: Saving a file
- **WHEN** generation 3 has an open connection and the user saves a change that applies as generation 4, whose module opens a new one
- **THEN** the server sees generation 3's connection closed with 1001, generation 4's connection opens, and generation 3's handlers receive no `close` event

#### Scenario: A reload that fails
- **WHEN** generation 3 has an open connection and the next save throws while evaluating after opening a connection
- **THEN** the failed load's connection is closed, generation 3's connection stays open and keeps receiving messages

#### Scenario: Saving during a fetch
- **WHEN** generation 3 has a fetch in flight and generation 4 is committed
- **THEN** the request is cancelled, and generation 3's promise neither resolves nor rejects

### Requirement: Bounded inbound queues and message size
Each connection SHALL hold at most 4,096 received messages, and at most 64 MiB of them, not yet delivered. A received message over 16 MiB, or one that would take the queue over either bound, SHALL close the connection with 1009 ("message too big"): the messages already queued SHALL be delivered, then `error` and `close` (1009, `wasClean` false), and the log SHALL say which bound was crossed. No message SHALL be dropped without that close. Outbound data SHALL be queued without a limit and counted in `bufferedAmount`, as the web does, which falls as the I/O side writes it.

#### Scenario: A burst after reconnect
- **WHEN** a server sends 2,000 small messages between two frames
- **THEN** all 2,000 are delivered in order in the next frame and the connection stays open

#### Scenario: A flood
- **WHEN** a server sends 5,000 small messages between two frames
- **THEN** the next frame delivers the first 4,096 in order, then `error` and `close` with 1009, and the log says the queue bound was crossed

#### Scenario: A huge message
- **WHEN** a server sends one 20 MiB message
- **THEN** `error` and `close` fire with 1009 and the log says the message was over 16 MiB

### Requirement: Networking is testable without a network
`bevy-ts` core SHALL drive connections and fetches only through a `Transport` trait, with a real transport (sockets) and a fake one in-process. Under the fake transport a test SHALL be able to accept or refuse an open, push text and binary messages, close from the server side with a code, drop the connection, read what the script sent, read each fetch's method, URL, headers and body, answer it with a status, headers and body, or fail it, with no socket and no thread. The fake SHALL apply the same allowlist, redirect, limits and generation rules as the real one, because those live above the seam. Tests of the real transport SHALL use only servers on the loopback interface: a TLS server's certificate SHALL come from a test certificate authority the test hands the transport, never the platform's trust store.

#### Scenario: A headless test of the abyme client's handshake
- **WHEN** a headless test installs the fake transport, loads a script that opens `ws://127.0.0.1:4477/api/stream`, accepts the open and pushes a hello message
- **THEN** the script's `open` and `message` handlers run in the frames that follow, and the test reads the script's first sent frame

#### Scenario: A headless test of a fetch
- **WHEN** a headless test installs the fake transport, loads a script that awaits `fetch("http://127.0.0.1:4477/api/held").then(r => r.json())`, and answers the request with 200 and `{"held":[]}`
- **THEN** the script's continuation runs in the frame after the answer with `{ held: [] }`

### Requirement: Networking runs only in play
Networking SHALL follow what runs scripts: edit mode runs no game code, so it has no connections and no fetches. While the engine is in edit mode, a `WebSocket` opened or a `fetch` called (by a module evaluating, or by anything else) SHALL be refused as a disallowed URL is, the log saying networking runs only in play. When the game leaves play for edit, the engine SHALL close every open connection with 1001 and deliver its `close` event on the next delivery in play, and SHALL cancel every fetch in flight and reject its promise with a `TypeError` on the next delivery in play. A reload in edit mode SHALL open no connection. `bevy-ts/pause` in play SHALL NOT close or hold connections or fetches. A shipped game, always in play, SHALL be unaffected.

#### Scenario: Opening the arena to edit
- **WHEN** the arena starts with `--edit` and its module opens an allowed connection when it evaluates
- **THEN** no connection is attempted and the log says networking runs only in play

#### Scenario: Back to edit
- **WHEN** a playing game has an open connection and a client sets `edit`
- **THEN** the server sees it closed with 1001, and when the client sets `play` the script's `close` handler runs with 1001 and its own reconnect logic may open a new one

#### Scenario: A fetch across edit
- **WHEN** a playing game has a fetch in flight and a client sets `edit`, then `play`
- **THEN** the request is cancelled, and at the first frame of play the promise rejects with a `TypeError`

#### Scenario: A Startup system connects
- **WHEN** the arena was started with `--edit` and a TS `Startup` system opens a connection
- **THEN** it connects at the first Play

### Requirement: Types for the globals
The `bevy-ts` package SHALL export `bevy-ts/globals`, a declaration file for every global the engine installs and nothing else. A project SHALL get them with `"types": ["bevy-ts/globals"]` and a `lib` without `dom`. `headers` SHALL be marked non-standard in its doc comment, and `Response.body` SHALL NOT be declared. `process` SHALL be declared through the `NodeJS.Process` and `NodeJS.ProcessEnv` interfaces, and `WebSocket`, `fetch`, `Request`, `Response`, `Headers`, `Blob` and the other classes so that a project whose `node_modules` holds `@types/node` still type-checks with no duplicate-declaration error. The examples' `tsconfig.json` SHALL use it.

#### Scenario: A project that also has @types/node
- **WHEN** a project's `tsconfig.json` names `bevy-ts/globals` and a package it imports references `@types/node`
- **THEN** `tsc --noEmit` passes, and `new WebSocket(url, { headers })`, `await (await fetch(url)).json()`, `new Blob(["x"])`, `process.argv[2]` and `process.env.ABYME_URL` type-check

### Requirement: Scripts get the web and Node globals a network client needs
The V8 backend SHALL install, before any module evaluates, the globals `WebSocket`, `fetch`, `Request`, `Response`, `Headers`, `Blob`, `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, `URL`, `URLSearchParams`, `AbortController`, `AbortSignal`, `EventTarget`, `Event`, `CloseEvent`, `MessageEvent`, `DOMException` and `process` (with `env` and `argv` only). Each SHALL behave as the WHATWG standard or, for `process`, Node's does, for the parts listed in these requirements. `AbortSignal` SHALL support `aborted`, `reason`, `throwIfAborted()`, `addEventListener("abort")`, `AbortSignal.abort()` and `AbortSignal.timeout()`, and `instanceof` SHALL hold for signals made by `AbortController`. `ReadableStream`, `FormData`, `File`, `TextEncoder`, `TextDecoder` and Node modules SHALL NOT be installed by this change.

#### Scenario: The abyme client builds its stream URL
- **WHEN** a module evaluates `new URL("/api/stream", "http://127.0.0.1:4477").toString()`
- **THEN** it yields `"http://127.0.0.1:4477/api/stream"` and no `ReferenceError` is raised

#### Scenario: Aborting a call
- **WHEN** a script makes an `AbortController`, listens for `abort` on its signal and calls `abort(new Error("stop"))`
- **THEN** the listener runs once, `signal.aborted` is true and `signal.reason.message` is `"stop"`

#### Scenario: Fetch is there
- **WHEN** a script reads `typeof fetch`, `typeof Blob` and `Array.isArray(process.argv)`
- **THEN** they are `"function"`, `"function"` and `true`

### Requirement: TLS for wss and https
`wss:` and `https:` SHALL connect over TLS 1.2 or 1.3 through rustls, verifying the server's certificate chain and name with the platform's verifier (the system trust store and its revocation checks on macOS and Windows; the system's certificate files on Linux). The server name SHALL be the URL's host, an IP address host verified against the certificate's IP names. A certificate the verifier refuses SHALL fail the connection: a `WebSocket` SHALL fire `error` then `close` with 1006, and a `fetch` SHALL reject with a `TypeError`; the log SHALL name the origin and why the certificate was refused. No option SHALL turn verification off. ALPN SHALL offer `http/1.1` only.

#### Scenario: A trusted local server
- **WHEN** a test runs a `wss:` echo server on `127.0.0.1` with a certificate from a test authority, hands that authority to the transport, and a script opens it and sends `"hi"`
- **THEN** `open` fires and `message` fires with `"hi"`

#### Scenario: An untrusted certificate
- **WHEN** the same server is used without handing the transport its authority
- **THEN** `error` and `close` with 1006 fire, and the log says the certificate was not trusted

#### Scenario: The wrong name
- **WHEN** the test server's certificate names only `localhost` and a script fetches `https://127.0.0.1:<port>/`
- **THEN** the promise rejects with a `TypeError` and the log says the certificate does not match `127.0.0.1`

### Requirement: fetch with the web's shape
`fetch(input, init?)` SHALL accept a URL string, a `URL` or a `Request`, and `init` with `method`, `headers`, `body`, `signal` and `redirect`; other `init` members SHALL be accepted and ignored. Only `http:` and `https:` SHALL be fetched; any other scheme SHALL reject with a `TypeError` before the policy runs. A body SHALL be a string, `ArrayBuffer`, typed array view, `Blob` or `URLSearchParams`, with the web's default `content-type` for each; a body with `GET` or `HEAD` SHALL throw a `TypeError`. The request SHALL be HTTP/1.1 and SHALL carry no `Origin` and no `Cookie` unless the script sets `Cookie`; the engine SHALL set `host`, `content-length`, `connection`, `transfer-encoding` and `accept-encoding: identity` itself and ignore a script's values for them, and SHALL send `accept: */*` and `user-agent: bevy-ts/<version>` unless the script sets them. The promise SHALL resolve once the whole response has arrived, with a `Response` whose `status`, `statusText`, `ok`, `headers`, `url`, `redirected` and `type` (`"basic"`) are set, and whose `text()`, `json()`, `arrayBuffer()`, `bytes()` and `blob()` read the body once (`bodyUsed`), a second read rejecting with a `TypeError`. `clone()` SHALL work before the body is read. `Response.body` SHALL NOT be provided. `Headers` SHALL be case-insensitive, iterate in sorted order, and combine repeated names with `", "` except `set-cookie`, which `getSetCookie()` returns apart. `Request` and `Response` SHALL be constructible as the web's are, with `Response.json()`, `Response.error()` and `Response.redirect()`. A network failure (DNS, refused, reset, TLS) SHALL reject with a `TypeError` whose message names the origin and the failure. Each fetch SHALL open its own connection, closed when the response is in.

#### Scenario: The abyme client reads what is held
- **WHEN** `net.allow` has `127.0.0.1:4477`, a script calls `await (await fetch("http://127.0.0.1:4477/api/held", { headers: { authorization: "Bearer k" } })).json()`, and the server answers 200 with `{"held":[]}`
- **THEN** the value is `{ held: [] }`, and the server saw `authorization: Bearer k`, `accept-encoding: identity` and no `origin`

#### Scenario: A POST with a JSON body
- **WHEN** a script fetches with `{ method: "POST", body: JSON.stringify({ a: 1 }) }`
- **THEN** the server sees `content-type: text/plain;charset=UTF-8`, `content-length: 7` and the body `{"a":1}`

#### Scenario: The body is read once
- **WHEN** a script calls `await res.text()` then `await res.text()` on one response
- **THEN** the first resolves to the body and the second rejects with a `TypeError`

#### Scenario: Nothing listening
- **WHEN** a script fetches an allowed `http://127.0.0.1:<port>/` where no server listens
- **THEN** the promise rejects with a `TypeError` naming `127.0.0.1:<port>`

### Requirement: Aborting a fetch
A `signal` in `init` or on the `Request` SHALL abort the fetch: when it is already aborted, `fetch` SHALL reject with its `reason` and open nothing; when it aborts while the request is in flight, the promise SHALL reject with its `reason` at once, without waiting for a delivery, and the engine SHALL cancel the request by the next delivery. A `reason` left unset SHALL be a `DOMException` named `AbortError`; `AbortSignal.timeout(ms)` SHALL reject with one named `TimeoutError`.

#### Scenario: A timeout
- **WHEN** a script calls `fetch(url, { signal: AbortSignal.timeout(50) })` to a server that never answers, at 16 ms frames
- **THEN** within the fourth frame the promise rejects with a `DOMException` named `TimeoutError`, and the server sees the connection closed

#### Scenario: Aborted before it starts
- **WHEN** a script aborts a controller, then calls `fetch(url, { signal })`
- **THEN** the promise rejects with an `AbortError` and no connection is attempted

### Requirement: Redirects stay inside net.allow
With `redirect` `"follow"` (the default), the engine SHALL follow `301`, `302`, `303`, `307` and `308` responses with a `location`, at most 20 times, resolving `location` against the current URL. Every hop SHALL pass the allowlist before its DNS lookup; a hop not allowed SHALL reject the fetch with a `TypeError` and log as a refused fetch does, naming the hop. A hop from `https:` to `http:` SHALL be refused the same way, the log saying a redirect may not leave TLS. A `303`, or a `301` or `302` to a `POST`, SHALL continue as a `GET` with no body; a `307` or `308` SHALL resend the method and body. A hop to another host or port SHALL drop the `authorization` and `cookie` headers. A twenty-first redirect SHALL reject with a `TypeError`. With `redirect` `"error"`, a redirect SHALL reject with a `TypeError`; with `"manual"`, the redirect response itself SHALL resolve, as Deno does. The final `Response` SHALL have `url` set to the last URL and `redirected` true when a redirect was followed.

#### Scenario: A redirect to a host not allowed
- **WHEN** `net.allow` is `["127.0.0.1:4477"]` and `http://127.0.0.1:4477/a` answers `302` with `location: http://10.0.0.9/b`
- **THEN** nothing connects to `10.0.0.9`, the promise rejects with a `TypeError`, and the log says `10.0.0.9:80` is not in `net.allow`

#### Scenario: A redirect that keeps its key
- **WHEN** both `127.0.0.1:4477` and `127.0.0.1:4478` are allowed and a fetch with `authorization` to `:4477/a` is redirected with `307` to `:4477/b`, then with `307` to `:4478/c`
- **THEN** `:4477/b` receives the `authorization` header, `:4478/c` does not, and the response's `url` is `http://127.0.0.1:4478/c` with `redirected` true

#### Scenario: Leaving TLS
- **WHEN** an allowed `https:` server answers `301` with an `http:` location on an allowed host
- **THEN** the promise rejects with a `TypeError` and the log says a redirect may not leave TLS

### Requirement: Bounded responses
A response body over 16 MiB SHALL reject the fetch with a `TypeError`, cancel the request, and log that the body was over the 16 MiB limit. The engine SHALL read no more than 16 MiB and a few kilobytes of it. A request body SHALL have no limit.

#### Scenario: A huge download
- **WHEN** an allowed server answers a fetch with a 20 MiB body
- **THEN** the promise rejects with a `TypeError` and the log names the 16 MiB limit

### Requirement: Blob
`new Blob(parts?, options?)` SHALL accept strings (as UTF-8), `ArrayBuffer`s, typed array views and `Blob`s, and `options.type`, lower-cased, or `""` when it holds a character outside printable ASCII. A `Blob` SHALL have `size`, `type`, `slice(start?, end?, contentType?)` with negative indexes counting from the end, and `text()`, `arrayBuffer()` and `bytes()` returning promises. Its bytes SHALL be copied when it is made, so a later change to a part does not change it. `stream()` SHALL NOT be provided.

#### Scenario: Building and slicing
- **WHEN** a script makes `new Blob(["héllo", new Uint8Array([33])], { type: "Text/Plain" })` and slices it with `slice(-2)`
- **THEN** the blob's `size` is 7 and `type` is `"text/plain"`, and the slice's `text()` resolves to `"o!"`

### Requirement: process.argv
`process.argv` SHALL be an array of strings in Node's shape: the engine executable's absolute path, then the entry's absolute path (the entry file under `bevy-ts dev`, the game's entry module under a shipped game or `bevy-ts play`), then the game's arguments. `bevy-ts dev <entry> [options] -- <args>…` SHALL pass the arguments after `--` and SHALL keep them across reloads and restarts of that run, including when the dev supervisor starts a project engine. A shipped game and `bevy-ts play` SHALL pass them as the shipped-game capability says. With no game arguments the array SHALL hold the first two entries only. It SHALL be set before any module evaluates.

#### Scenario: Arguments under dev
- **WHEN** the operator runs `bevy-ts dev src/main.ts -- --level arena`
- **THEN** `process.argv.slice(2)` is `["--level", "arena"]` and `process.argv[1]` ends with `src/main.ts`

#### Scenario: No arguments
- **WHEN** the operator runs `bevy-ts dev src/main.ts`
- **THEN** `process.argv.length` is 2
