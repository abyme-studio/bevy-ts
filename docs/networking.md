# Script networking

A script can open a `WebSocket` to an allowed host, set timers and read the environment variables
the project names. It is enough to run `@abyme/client`'s `connect()` unchanged against abymed on
the same Mac. The code is `crates/bevy-ts/src/net/` (connections, policy, limits) and
`crates/bevy-ts-v8/js/web.js` (the globals).

## The globals

The engine installs these before any module runs. Each has the web's shape, and `process.env`
has Node's.

- `WebSocket`, `CloseEvent`, `MessageEvent`, `Event`, `EventTarget`, `DOMException`
- `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval` (ids are positive integers)
- `URL`, `URLSearchParams`
- `AbortController`, `AbortSignal` (with `abort()`, `timeout()` and `any()`)
- `process`, with `env` only

Missing: `fetch`, `wss:`, `Blob`, `process.argv` and Node modules. `typeof fetch` is
`"undefined"`. `wss:` needs a TLS crate and comes later; until then the only target is loopback.

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
- The host is checked before any DNS lookup. An empty allowlist refuses everything.

Started with `ABYME_URL=http://127.0.0.1:5000`, the example allows `127.0.0.1:4477` and
`127.0.0.1:5000`. `process.env.ABYME_URL` is that URL, and `process.env.ABYME_KEY` and
`process.env.HOME` are `undefined`.

`bevy-ts dev` reads both when it starts the engine. An edit to them applies at the next engine
start: restart the engine (the studio's restart does it), since a hot reload does not.
`bevy-ts build` writes both to `game/game.json` as written, and a shipped game expands them from
its own environment when it starts ([build.md](build.md)).

## Refusals

A refused open never touches the network. The constructor still succeeds. At the next delivery
`error` fires, then `close` with 1006, and the log gets a warning:

| case | log line |
| --- | --- |
| host not allowed | `WebSocket ws://example.com/: example.com:80 is not in net.allow; add it to net.allow in bevy-ts.json` |
| `wss:` | `WebSocket wss://…: wss: is not supported yet; use ws: to a loopback host` |
| `headers` to a host that is not loopback | `WebSocket …: headers are sent only over loopback until wss: is supported` |
| `headers` naming `Origin` | `WebSocket …: the engine sends no Origin header; remove it from headers` |
| edit mode | `WebSocket …: networking runs only in play; open it from a system or once the game plays` |
| not a `ws:` URL | `WebSocket …: not a WebSocket URL: …` |

Plain bad input (a relative URL, a fragment, repeated protocols) throws `SyntaxError` from the
constructor, as the web does. An `http:` URL is read as `ws:`.

## The WebSocket

The standard client: `new WebSocket(url, protocols?)`, text frames as strings and binary frames as
`ArrayBuffer`s, `send`, `close`, `bufferedAmount`, and `open`, `message`, `error` and `close`
through `onX` or `addEventListener`.

- `binaryType` is always `"arraybuffer"`. Setting `"blob"` logs a warning and keeps it.
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

It is refused unless the host is loopback (`localhost`, `127.0.0.0/8`, `::1`), so a key never
crosses the network in clear text. Nothing in bevy-ts depends on it; a browser has no such
option.

The handshake sends no `Origin` header, as Deno does: a script has no page origin, and abymed
answers `403` to a foreign one. A `headers` option naming `Origin` is refused.

## Timers

Timers run on real time (`Time<Real>`), so they keep running while the game is paused, as the
web's do. A timer fires at the first delivery at or after its deadline: one frame is the
granularity. Due timers fire in deadline order, ties in the order they were set. An interval
delayed by a long frame fires once and reschedules from then, with no catch-up.

## Delivery

Socket I/O runs off the main thread. Once a frame, in `First` after time advances, the engine
hands JS every socket event since the last frame and fires every due timer, in one call. When
nothing is pending and no timer is due, it makes no call. Callbacks run outside any system, so a
Rust param call made in one throws the usual "only while a system runs" error; set state there and
let a system act on it.

## Play only

Edit mode runs no game code, so it has no connections and fires no timers.

- An open in edit mode is refused (the log says networking runs only in play). A level opened with
  `--edit` therefore connects nothing on load; a `Startup` system that connects does so at the
  first play.
- Leaving play for edit closes every connection with 1001. The script's `close` handlers run at
  the first frame of play, where its own reconnect logic can open a new one.
- A timer due during edit fires at the first frame of play.
- Pausing in play closes nothing and holds nothing. A shipped game is always in play.

## Reloads

Each connection and timer belongs to the generation that made it.

- A save that commits a new generation closes the old one's connections with 1001 and drops its
  timers. The old handlers get no `close` event. The new code opens its own.
- A load that fails or is discarded closes only the connections and timers it made. The live
  generation keeps its connections and keeps receiving messages.
- A restart (or an engine replaced for heap size) closes every connection and drops every timer.
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
and falls as it is written.

## Testing

Tests need no network. `bevy_ts::net::install_fake(&mut app)` swaps the socket transport for a
`FakeTransport` and returns its handle. The allowlist, limits and generation rules sit above the
transport, so the fake is held to them too. Insert a `NetPolicy` to allow hosts, and drive real
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
