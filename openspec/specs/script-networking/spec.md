# script-networking Specification

## Purpose
TBD - created by archiving change script-networking. Update Purpose after archive.

## Requirements

### Requirement: Scripts get the web globals a network client needs
The V8 backend SHALL install, before any module evaluates, the globals `WebSocket`, `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, `URL`, `URLSearchParams`, `AbortController`, `AbortSignal`, `EventTarget`, `Event`, `CloseEvent`, `MessageEvent`, `DOMException` and `process` (with `env` only). Each SHALL behave as the WHATWG standard or, for `process.env`, Node's does, for the parts listed in these requirements. `AbortSignal` SHALL support `aborted`, `reason`, `throwIfAborted()`, `addEventListener("abort")`, `AbortSignal.abort()` and `AbortSignal.timeout()`, and `instanceof` SHALL hold for signals made by `AbortController`. `fetch`, `Blob`, `process.argv` and Node modules SHALL NOT be installed by this change.

#### Scenario: The abyme client builds its stream URL
- **WHEN** a module evaluates `new URL("/api/stream", "http://127.0.0.1:4477").toString()`
- **THEN** it yields `"http://127.0.0.1:4477/api/stream"` and no `ReferenceError` is raised

#### Scenario: Aborting a call
- **WHEN** a script makes an `AbortController`, listens for `abort` on its signal and calls `abort(new Error("stop"))`
- **THEN** the listener runs once, `signal.aborted` is true and `signal.reason.message` is `"stop"`

#### Scenario: No fetch yet
- **WHEN** a script reads `typeof fetch`
- **THEN** it is `"undefined"`

### Requirement: A WebSocket client with the web's shape
`new WebSocket(url, protocols?)` SHALL accept a `ws:` URL (`http:` is read as `ws:`, as the web does) and, as the web does, a protocol string or an array of them. As a marked, Deno-shaped extension, the second argument MAY instead be `{ protocols, headers }`; nothing in bevy-ts or its examples SHALL depend on `headers`, and the types SHALL document it as non-standard. The object SHALL have `url`, `protocol`, `extensions`, `readyState` (`CONNECTING` 0, `OPEN` 1, `CLOSING` 2, `CLOSED` 3, as instance and static constants), `bufferedAmount` and `binaryType`; `send(data)` with a string, `ArrayBuffer` or typed array view; `close(code?, reason?)`; events `open`, `message` (`MessageEvent` with `data`), `error` (a plain `Event`) and `close` (`CloseEvent` with `code`, `reason`, `wasClean`), through both `onX` properties and `addEventListener`. Text frames SHALL arrive as strings and binary frames as `ArrayBuffer`. `binaryType` SHALL default to `"arraybuffer"`; setting `"blob"` SHALL log a warning and keep `"arraybuffer"`. `close` SHALL throw `InvalidAccessError` for a code other than 1000 or 3000-4999 and `SyntaxError` for a reason over 123 UTF-8 bytes. A dropped connection SHALL report code 1006 with `wasClean` false; a close frame without a code SHALL report 1005. `send` while `CONNECTING` SHALL throw `InvalidStateError`; `send` after close SHALL be ignored, as the web does.

#### Scenario: Echo
- **WHEN** a script opens `ws://127.0.0.1:9001/echo` (allowed) to a server that echoes, and on `open` sends `"hi"` and a 3-byte `Uint8Array`
- **THEN** `message` fires twice, first with `data === "hi"`, then with an `ArrayBuffer` of those 3 bytes

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
The engine SHALL check every `WebSocket` URL against the allowlist before any DNS lookup or socket. An allowlist entry SHALL be `host` (any port), `host:port`, or a URL whose host and port are taken; host names compare case-insensitively and literally (`localhost` does not allow `127.0.0.1`). A refused URL SHALL open no socket: the constructor SHALL succeed, then `error` and `close` (1006) SHALL fire on the next delivery, and the log SHALL name the origin and say to add it to `net.allow` in `bevy-ts.json`. A `wss:` URL SHALL be refused the same way, the log saying `wss:` is not supported yet. A URL with a `headers` option whose host is not loopback (`localhost`, `127.0.0.0/8`, `::1`) SHALL be refused the same way, the log saying headers are sent only over loopback until `wss:` is supported. An empty allowlist SHALL refuse everything.

#### Scenario: Not allowed
- **WHEN** `net.allow` is `["127.0.0.1:4477"]` and a script opens `ws://example.com/`
- **THEN** no connection is attempted, `close` fires with 1006, and the log says `example.com` is not in `net.allow`

#### Scenario: A key to another machine
- **WHEN** `net.allow` has `10.0.0.5:4477` and a script opens `ws://10.0.0.5:4477/` with `{ headers: { authorization: "Bearer k" } }`
- **THEN** no connection is attempted, `close` fires with 1006, and the log says headers are sent only over loopback

#### Scenario: TLS
- **WHEN** a script opens `wss://127.0.0.1:4477/` and the host is allowed
- **THEN** no connection is attempted and the log says `wss:` is not supported yet

### Requirement: Network and environment settings in the project file
`bevy-ts.json` SHALL accept `net.allow`, an array of allowlist entries, and `env`, an array of environment variable names. An entry of the form `${NAME}` SHALL be read from the environment when the engine starts; an unset or empty variable SHALL add nothing. `process.env` SHALL show exactly the names in `env` that are set, read when the engine starts, and nothing else; it SHALL be the same frozen-in-content object for the whole run. Both default to empty. `bevy-ts dev` SHALL read them when it starts the engine; a change to them SHALL apply when the engine next starts. `bevy-ts build` SHALL write both, unexpanded, into `game/game.json` as `net` and `env`, and a shipped game SHALL read them from there, expanding `${NAME}` from its own environment at start. A `game.json` without them SHALL mean both empty.

#### Scenario: The abyme client finds abymed
- **WHEN** `bevy-ts.json` has `"net": { "allow": ["127.0.0.1:4477", "${ABYME_URL}"] }` and `"env": ["ABYME_URL", "ABYME_KEY"]`, and the engine starts with `ABYME_URL=http://127.0.0.1:5000` and `HOME` set
- **THEN** a script may connect to `127.0.0.1:4477` and `127.0.0.1:5000`, `process.env.ABYME_URL` is `"http://127.0.0.1:5000"`, `process.env.ABYME_KEY` is `undefined`, and `process.env.HOME` is `undefined`

#### Scenario: A built game
- **WHEN** that project is built and the built game starts with `ABYME_URL` unset
- **THEN** `game/game.json` holds `"${ABYME_URL}"` as written, and the game may connect only to `127.0.0.1:4477`

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
Each connection and timer SHALL belong to the generation that made it: the loading one while a module evaluates, else the one whose code made the call (a system, an entry, or a callback of a connection or timer of that generation). When a generation is committed, the engine SHALL close the previous generation's open connections with 1001 ("going away") and drop its timers, delivering no event to them. When a load fails or is discarded, the engine SHALL close the connections and drop the timers that load made, and touch nothing of the live generation. A `restart` reload or an engine instance replaced for heap size SHALL close every connection and drop every timer. A failed compile SHALL touch nothing.

#### Scenario: Saving a file
- **WHEN** generation 3 has an open connection and the user saves a change that applies as generation 4, whose module opens a new one
- **THEN** the server sees generation 3's connection closed with 1001, generation 4's connection opens, and generation 3's handlers receive no `close` event

#### Scenario: A reload that fails
- **WHEN** generation 3 has an open connection and the next save throws while evaluating after opening a connection
- **THEN** the failed load's connection is closed, generation 3's connection stays open and keeps receiving messages

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
`bevy-ts` core SHALL drive connections only through a `Transport` trait, with a real transport (sockets) and a fake one in-process. Under the fake transport a test SHALL be able to accept or refuse an open, push text and binary messages, close from the server side with a code, drop the connection, and read what the script sent, with no socket and no thread. The fake SHALL apply the same allowlist, limits and generation rules as the real one, because those live above the seam.

#### Scenario: A headless test of the abyme client's handshake
- **WHEN** a headless test installs the fake transport, loads a script that opens `ws://127.0.0.1:4477/api/stream`, accepts the open and pushes a hello message
- **THEN** the script's `open` and `message` handlers run in the frames that follow, and the test reads the script's first sent frame

### Requirement: Networking runs only in play
Networking SHALL follow what runs scripts: edit mode runs no game code, so it has no connections. While the engine is in edit mode, a `WebSocket` opened (by a module evaluating, or by anything else) SHALL be refused as a disallowed URL is, the log saying networking runs only in play. When the game leaves play for edit, the engine SHALL close every open connection with 1001 and deliver its `close` event on the next delivery in play. A reload in edit mode SHALL open no connection. `bevy-ts/pause` in play SHALL NOT close or hold connections. A shipped game, always in play, SHALL be unaffected.

#### Scenario: Opening the arena to edit
- **WHEN** the arena starts with `--edit` and its module opens an allowed connection when it evaluates
- **THEN** no connection is attempted and the log says networking runs only in play

#### Scenario: Back to edit
- **WHEN** a playing game has an open connection and a client sets `edit`
- **THEN** the server sees it closed with 1001, and when the client sets `play` the script's `close` handler runs with 1001 and its own reconnect logic may open a new one

#### Scenario: A Startup system connects
- **WHEN** the arena was started with `--edit` and a TS `Startup` system opens a connection
- **THEN** it connects at the first Play

### Requirement: Types for the globals
The `bevy-ts` package SHALL export `bevy-ts/globals`, a declaration file for every global this change installs and nothing else. A project SHALL get them with `"types": ["bevy-ts/globals"]` and a `lib` without `dom`. `headers` SHALL be marked non-standard in its doc comment. `process` SHALL be declared through the `NodeJS.Process` and `NodeJS.ProcessEnv` interfaces, and `WebSocket` and the other classes so that a project whose `node_modules` holds `@types/node` still type-checks with no duplicate-declaration error. The examples' `tsconfig.json` SHALL use it.

#### Scenario: A project that also has @types/node
- **WHEN** a project's `tsconfig.json` names `bevy-ts/globals` and a package it imports references `@types/node`
- **THEN** `tsc --noEmit` passes, and `new WebSocket(url, { headers })` and `process.env.ABYME_URL` type-check
