# Script networking

A script can open a `WebSocket` (`ws:` or `wss:`) or `fetch` (`http:` or `https:`) to an allowed
host, set timers, make `Blob`s, and read its arguments and the environment variables the project
names. It is enough to run `@abyme/client` unchanged against an abymed on the same Mac or, over
TLS, a remote one. The code is `crates/bevy-ts/src/net/` (connections, fetches, policy, redirects,
limits, TLS) and `crates/bevy-ts-v8/js/web.js` (the globals).

## The globals

The engine installs these before any module runs. Each has the web's shape, and `process` has
Node's.

- `fetch`, `Request`, `Response`, `Headers`
- `WebSocket`, `CloseEvent`, `MessageEvent`, `Event`, `EventTarget`, `DOMException`
- `Blob`
- `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval` (ids are positive integers)
- `URL`, `URLSearchParams`
- `AbortController`, `AbortSignal` (with `abort()`, `timeout()` and `any()`)
- `process`, with `env` and `argv`

Missing: `ReadableStream` (so no `Response.body`), `FormData`, `File`, `TextEncoder`,
`TextDecoder` and Node modules.

Types come from `bevy-ts/globals`:

```jsonc
// tsconfig.json
{ "compilerOptions": { "lib": ["es2023"], "types": ["bevy-ts/globals"] } }
```

Leave `dom` out of `lib`: it declares globals the engine does not have. The file merges with
`@types/node` when a package you import pulls it in. If you name both in `types`, put `node`
first.

## Allowing hosts and variables

Nothing is allowed by default. `bevy-ts.json` names the hosts a script may connect to and the
environment variables `process.env` shows:

```jsonc
{
  "net": { "allow": ["127.0.0.1:4477", "${ABYME_URL}"] },
  "env": ["ABYME_URL", "ABYME_KEY"]
}
```

- An entry is `host` (any port), `host:port`, or a URL, whose host and port are taken
  (`http://127.0.0.1:5000` allows `127.0.0.1:5000`; a URL without a port takes its scheme's).
- Hosts compare case-insensitively and literally: `localhost` does not allow `127.0.0.1`.
- `${NAME}` is read from the environment when the engine starts. An unset or empty variable adds
  nothing.
- `process.env` holds exactly the names in `env` that were set when the engine started. It does
  not change during the run.
- An entry's scheme picks its default port and nothing else: `ws://h` allows `https://h:80/` too.
- The host is checked before any DNS lookup, for every `WebSocket`, every `fetch` and every
  redirect hop. What an allowed name resolves to is not checked: the list says which names you
  trust. An empty allowlist refuses everything.

Started with `ABYME_URL=http://127.0.0.1:5000`, the example allows `127.0.0.1:4477` and
`127.0.0.1:5000`. `process.env.ABYME_URL` is that URL, and `process.env.ABYME_KEY` and
`process.env.HOME` are `undefined`.

`bevy-ts dev` reads both when it starts the engine. An edit to them applies at the next engine
start: restart the engine (the studio's restart does it), since a hot reload does not.
`bevy-ts build` writes both to `game/game.json` as written, and a shipped game expands them from
its own environment when it starts ([build.md](build.md)).

## Refusals

A refused `WebSocket` or `fetch` never touches the network. The `WebSocket` constructor still
succeeds; at the next delivery `error` fires, then `close` with 1006. A refused `fetch` rejects
with a `TypeError` at the next delivery, its message the log line's. The log gets a warning:

| case | log line |
| --- | --- |
| host not allowed | `WebSocket ws://example.com/: example.com:80 is not in net.allow; add it to net.allow in bevy-ts.json` |
| a fetch's host not allowed | `fetch https://example.com/: example.com:443 is not in net.allow; add it to net.allow in bevy-ts.json` |
| a redirect hop not allowed | `fetch http://10.0.0.9/b: 10.0.0.9:80 is not in net.allow; …` (the hop's URL) |
| a redirect from `https:` to `http:` | `fetch http://…: a redirect may not leave TLS` |
| `headers` over `ws:` to a host that is not loopback | `WebSocket …: headers are sent in clear only over loopback; use wss:` |
| `headers` naming `Origin` | `WebSocket …: the engine sends no Origin header; remove it from headers` |
| edit mode | `WebSocket …: networking runs only in play; open it from a system or once the game plays` (`fetch …:` for a fetch) |
| not a `ws:`/`wss:` (or `http:`/`https:`) URL | `WebSocket …: not a URL the engine connects to: …` |

A connection or fetch that was allowed but failed logs why, by origin: `WebSocket to
127.0.0.1:4477: the connection failed: …`, `fetch https://…: the certificate was not trusted …`.

Plain bad input (a relative URL, a fragment, repeated protocols) throws `SyntaxError` from the
`WebSocket` constructor, as the web does. An `http:` URL is read as `ws:`, `https:` as `wss:`.

## TLS

`wss:` and `https:` run over rustls (TLS 1.2 and 1.3, the `ring` provider). The certificate is
verified by the platform: Security.framework on macOS (with its revocation checks), CryptoAPI on
Windows (likewise), the system's certificate files on Linux (no revocation check, as browsers
there). So a CA the player's machine trusts, a corporate or school one, works, and the roots update
with the OS for the life of a shipped game. The server name is the URL's host; an IP host is
checked against the certificate's IP names. ALPN offers `http/1.1` only.

There is no option to turn verification off. A refused certificate fails the connection
(`error`, then `close` 1006) or the fetch (a `TypeError`), and the log says why: not trusted,
does not match the host, expired.

`ring` has no post-quantum key exchange: `X25519MLKEM768` needs the `aws-lc-rs` provider, a much
larger C build. The classic key exchanges are used.

## The WebSocket

The standard client: `new WebSocket(url, protocols?)`, text frames as strings and binary frames as
`ArrayBuffer`s, `send`, `close`, `bufferedAmount`, and `open`, `message`, `error` and `close`
through `onX` or `addEventListener`.

- `binaryType` is `"arraybuffer"` by default, unlike the web's `"blob"`, so stage 1's scripts keep
  working. Set it to `"blob"` and a binary message arrives as a `Blob` with an empty `type`. Any
  other value is ignored.
- `send` takes a string, an `ArrayBuffer`, a typed array view or a `Blob` (one binary frame, in
  order with the other sends, counted in `bufferedAmount`).
- `close` takes 1000 or 3000-4999 (else `InvalidAccessError`) and a reason of at most 123 UTF-8
  bytes (else `SyntaxError`). `send` while connecting throws `InvalidStateError`; after close it
  is ignored.
- A connection dropped without a close frame reports 1006 with `wasClean` false. A close frame
  without a code reports 1005.
- A `send` reaches the socket at the next frame's delivery, at most one frame later.

### The `headers` option

Non-standard, as Deno has it: the second argument may be `{ protocols, headers }`.

```ts
new WebSocket("ws://127.0.0.1:4477/api/stream", { headers: { authorization: `Bearer ${key}` } });
```

Over `wss:` it is sent to any allowed host. Over `ws:` it is refused unless the host is loopback
(`localhost`, `127.0.0.0/8`, `::1`), so a key never crosses the network in clear text. Nothing in
bevy-ts depends on it; a browser has no such option.

The handshake sends no `Origin` header, as Deno does: a script has no page origin, and abymed
answers `403` to a foreign one. A `headers` option naming `Origin` is refused.

## fetch

The web's `fetch(input, init?)`, with `Request`, `Response` and `Headers`, for `http:` and
`https:`:

```ts
const res = await fetch("http://127.0.0.1:4477/api/held", { headers: { authorization: `Bearer ${key}` } });
const { held } = await res.json();
```

- `init` takes `method`, `headers`, `body`, `signal` and `redirect`; the rest is accepted and
  ignored. A body is a string, `ArrayBuffer`, typed array view, `Blob` or `URLSearchParams`, with
  the web's default `content-type`. A body with `GET` or `HEAD` rejects with a `TypeError`.
- HTTP/1.1 only. The engine sets `host`, `content-length`, `connection: close`,
  `accept-encoding: identity` and ignores a script's values for them, and for `transfer-encoding`.
  It sends `accept: */*` and `user-agent: bevy-ts/<version>` unless the script sets them. No
  `Origin`, no cookie jar (a `cookie` header the script sets is sent), no CORS: a script has no
  origin.
- The promise resolves once the whole response has arrived, not at its headers. `text()`,
  `json()`, `arrayBuffer()`, `bytes()` and `blob()` read the body once; a second read rejects
  with a `TypeError`. `clone()` works before a read. There is no `Response.body` stream.
- `Headers` iterate sorted by name, combine repeated names with `", "`, and keep each
  `set-cookie` apart (`getSetCookie()`).
- `signal` aborts it: already aborted, the fetch rejects with its `reason` and opens nothing;
  aborted in flight, the promise rejects at once and the engine cancels the request by the next
  frame. `AbortSignal.timeout(ms)` rejects with a `TimeoutError`.
- A network failure (DNS, refused, reset, TLS) rejects with a `TypeError` naming the origin and
  the failure: `fetch 127.0.0.1:5000: the connection failed: Connection refused`.

What it does not do:

| | why |
| --- | --- |
| no connection reuse | each fetch opens its own connection (and TLS handshake) and closes it when the response is in; fine for a game's occasional calls |
| bodies are buffered whole, at most 16 MiB | a response over 16 MiB rejects with a `TypeError`, cancels the request, and logs `fetch …: the body was over the 16 MiB limit`; the engine reads at most 16 MiB and a few kilobytes of it. A request body has no limit |
| no cap on fetches in flight | each is bounded by its 16 MiB; a cap waits for a need |
| proxies are ignored | `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` are not read |
| no HTTP/2, no compression, no streaming | `accept-encoding: identity`; `ReadableStream` is not installed |

### Redirects

With `redirect: "follow"` (the default) the engine follows `301`, `302`, `303`, `307` and `308`
with a `location`, at most 20 times, inside `net.allow`:

- Every hop passes the allowlist before its DNS lookup. A hop not allowed rejects the fetch and
  logs the hop's URL.
- A hop from `https:` to `http:` is refused: a redirect may not leave TLS.
- A `303`, or a `301` or `302` to a `POST`, continues as a `GET` with no body (and none of its
  `content-*` headers). A `307` or `308` resends the method and body.
- A hop to another host or port drops `authorization` and `cookie`.
- The twenty-first redirect rejects with a `TypeError`.

`redirect: "error"` rejects on a redirect. `redirect: "manual"` resolves with the redirect
response itself, as Deno does. The final `Response` has `url` set to the last URL and
`redirected` true when a redirect was followed. Redirects are followed in core, above the
transport, so the fake transport sees each hop as a fetch of its own.

## Blob

`new Blob(parts?, { type? })` takes strings (as UTF-8), `ArrayBuffer`s, typed array views and
`Blob`s, and copies their bytes when it is made. `type` is lower-cased, or `""` when it holds a
character outside printable ASCII. It has `size`, `type`, `slice(start?, end?, contentType?)`
(negative indexes count from the end) and `text()`, `arrayBuffer()` and `bytes()`. No `stream()`.
It lives in memory.

## process.argv

Node's shape: the engine executable's absolute path, the entry's absolute path, then the game's
arguments.

| how it runs | `argv[1]` | `argv.slice(2)` |
| --- | --- | --- |
| `bevy-ts dev src/main.ts -- --level arena` | `…/src/main.ts` | `["--level", "arena"]` |
| `bevy-ts play dist/arena/game -- --seed 7` | `…/game/scripts/src/main.ts` | `["--seed", "7"]` |
| `dist/…/arena --server 10.0.0.5` (shipped) | `…/game/scripts/src/main.ts` | `["--server", "10.0.0.5"]` |

With no game arguments it holds the first two. It is set before any module evaluates and kept
across reloads and restarts of the run, including when `bevy-ts dev` runs a project engine. A
shipped game passes every argument it is started with ([build.md](build.md)).

## Timers

Timers run on real time (`Time<Real>`), so they keep running while the game is paused, as the
web's do. A timer fires at the first delivery at or after its deadline: one frame is the
granularity. Due timers fire in deadline order, ties in the order they were set. An interval
delayed by a long frame fires once and reschedules from then, with no catch-up.

## Delivery

Socket and fetch I/O run off the main thread. Once a frame, in `First` after time advances, the
engine hands JS every socket event and every finished fetch since the last frame and fires every
due timer, in one call. When
nothing is pending and no timer is due, it makes no call. Callbacks run outside any system, so a
Rust param call made in one throws the usual "only while a system runs" error; set state there and
let a system act on it.

## Play only

Edit mode runs no game code, so it has no connections or fetches and fires no timers.

- An open or a fetch in edit mode is refused (the log says networking runs only in play). A level
  opened with `--edit` therefore connects nothing on load; a `Startup` system that connects does
  so at the first play.
- Leaving play for edit closes every connection with 1001 and cancels every fetch. The script's
  `close` handlers run, and its fetches reject with `TypeError: the game left play`, at the first
  frame of play, where its own reconnect logic can try again.
- A timer due during edit fires at the first frame of play.
- Pausing in play closes nothing and holds nothing. A shipped game is always in play.

## Reloads

Each connection, fetch and timer belongs to the generation that made it.

- A save that commits a new generation closes the old one's connections with 1001, cancels its
  fetches and drops its timers. The old handlers get no `close` event, and the old fetches'
  promises never settle. The new code opens its own.
- A load that fails or is discarded closes only the connections, fetches and timers it made. The
  live generation keeps its connections and keeps receiving messages.
- A restart (or an engine replaced for heap size) closes every connection, cancels every fetch
  and drops every timer.
- A compile error touches nothing.

## Limits

A connection holds at most 4,096 received messages, and at most 64 MiB of them, not yet
delivered. A message over 16 MiB, or one past either bound, closes the connection with 1009
("message too big"). What was already queued is delivered first, then `error` and `close` (1009,
`wasClean` false), and the log names the bound:

```
WebSocket to 127.0.0.1:4477 closed with 1009: more than 4096 messages arrived before the game could take them
WebSocket to 127.0.0.1:4477 closed with 1009: more than 64 MiB arrived before the game could take it
WebSocket to 127.0.0.1:4477 closed with 1009: a message of 20971520 bytes is over the 16 MiB limit
```

No message is dropped without that close. Outbound data has no limit; `bufferedAmount` counts it
and falls as it is written. A fetch's response has its own 16 MiB bound (fetch, above).

## Testing

Tests need no network. `bevy_ts::net::install_fake(&mut app)` swaps the socket transport for a
`FakeTransport` and returns its handle. The allowlist, limits, redirects and generation rules sit
above the transport, so the fake is held to them too. Insert a `NetPolicy` to allow hosts, and drive real
time by hand so timers are deterministic:

```rust
use bevy::time::TimeUpdateStrategy;
use bevy_ts::net::{Frame, NetPolicy};

app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(16)));
let (policy, _) = NetPolicy::from_config(&["127.0.0.1:4477".into()], &[], &|_| None);
app.insert_resource(policy);
let fake = bevy_ts::net::install_fake(&mut app);

// load a script that opens ws://127.0.0.1:4477/api/stream, then:
app.update();
let (id, conn) = fake.last().unwrap();
fake.accept(id, "");
fake.push_text(id, r#"{"hello":1}"#);
app.update();
assert_eq!(fake.conn(id).unwrap().sent[0], Frame::Text("…".into()));
```

The handle can also `refuse` an open, `push_binary`, `server_close(id, Some(code), reason)` (or
`None` for a close frame without a code, seen as 1005), `drop_conn` (seen as 1006), report
`written` bytes, and read each connection's handshake (`request.headers`, `request.protocols`),
sent frames and the close the script or engine asked for. At 16 ms a frame, `setTimeout(f, 50)`
fires in the fourth frame after it was set.

Fetches, each redirect hop its own:

```rust
// load a script that awaits fetch("http://127.0.0.1:4477/api/held").then((r) => r.json()), then:
app.update();
let (id, hop) = fake.last_fetch().unwrap();
assert_eq!(hop.request.url, "http://127.0.0.1:4477/api/held"); // and .method, .headers, .body
fake.respond(id, 200, &[("content-type", "application/json")], br#"{"held":[]}"#);
app.update(); // the script's continuation runs
```

`fake.fail(id, reason)` fails one as a network error would, and `fake.cancelled(id)` says whether
the engine cancelled it (an abort, a save, edit mode).

A test of the real transport against a local TLS server hands the transport its own authority:
`bevy_ts::net::install_socket(&mut app, NetTls::with_roots(pem)?)`. Nothing in `bevy-ts.json`
sets it. The tests' authority and certificates are in `crates/bevy-ts/tests/it/fixtures/tls/`.
