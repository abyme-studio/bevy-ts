## Context

Research: `research.md` in this change. The engine installs only `console` (`crates/bevy-ts-v8/js/core.js:81`); deno_core 0.412 ships no timers, `URL` or `AbortController`, and Deno's `deno_web`/`deno_url`/`deno_websocket` are ruled out. `pump` in `First` polls the event loop once with a no-op waker (`bevy-ts-v8/src/lib.rs:566`). Host calls work only inside a system (`lib.rs:67`). Generations load, then commit or discard (`crates/bevy-ts/src/generation.rs:139`); a restart or a heap over 1 GiB makes a fresh isolate. The CLI runs the engine in-process and sets engine resources from `bevy-ts.json` before the run (`cli.rs:283`); a shipped game reads only `game/game.json` (`play.rs`).

The target is `@abyme/client`'s `connect()`, run unchanged against abymed on the same Mac, from a game whose npm import `script-packages` resolves at compile time.

The ask-first items were settled by lead@script-networking for agent town on 2026-10-23 (proposal, Impact). This design follows them.

## Goals / Non-Goals

**Goals:**
- `connect()` runs unchanged: `WebSocket`, timers, `URL`, `AbortController`, `process.env`.
- Socket I/O off the main thread; at most one JS call a frame for networking, none when idle.
- A reload never leaks a socket or a timer; a failed reload disturbs nothing.
- Headless, deterministic tests through a fake transport.

**Non-Goals:** `wss:`/TLS, `fetch`, `Blob`, `process.argv`, Node modules, a server socket, networking in edit mode, a protocol change, hot-applying `net`/`env` edits without an engine restart.

## Decisions

### 1. Registry and transport in core; plain data across the backend seam

`crates/bevy-ts/src/net/` owns a `NonSend` `NetRegistry`: connections by id, each with its generation, state and inbound queue; the `NetPolicy` resource (allowlist, visible env); and a `Transport` trait:

```rust
trait Transport {
    fn open(&mut self, id: ConnId, req: &OpenRequest);     // url, protocols, headers
    fn send(&mut self, id: ConnId, data: Frame);
    fn close(&mut self, id: ConnId, code: u16, reason: &str);
    fn poll(&mut self, out: &mut Vec<NetEvent>);           // drains what arrived
}
```

The backend holds no socket. Its ops (`op_bevyts_ws_open`, `op_bevyts_ws_send`, `op_bevyts_ws_close`) append requests to an outbox in `OpState`; ids are allocated in JS. Core drains the outbox through the seam.

Why: the seam stays engine-neutral (script-runtime), and the allowlist, limits and generation rules live once, above the transport, so the fake transport tests them too. Alternative: ops call into a core-owned registry through a trait object handed to the backend. Rejected: it couples the backend to core's registry and makes refusals synchronous, which is not the web's shape. Cost: a send made in a system reaches the socket at the next `First`, at most one frame later. Acceptable for a game client.

### 2. One seam call a frame: `deliver`

`ScriptBackend` gains:

```rust
fn deliver(&mut self, now_ms: f64, events: &[NetEvent]) -> Result<Delivered, ScriptError>;
// Delivered { requests: Vec<NetRequest>, next_timer_ms: Option<f64> }
```

The V8 backend calls JS (`__bevyts.deliver(now, batch)`) only when `events` is non-empty or `now_ms >= next_timer_ms`; otherwise it only drains its outbox. JS dispatches socket events in arrival order, then fires due timers, then returns its next deadline. Events cross as one array of records with binary payloads as `ArrayBuffer`s, not one call per message. `Delivered.requests` holds what scripts asked since the last drain; core applies them to the transport the same frame.

`load` also drains the outbox after evaluation, so a module's top-level `new WebSocket` reaches core tagged with the loading generation, and the visible env goes in on `Generation` (`Generation { number, set, env }`), so `process.env` exists before any module runs.

The fake backend implements `deliver` by recording calls.

### 3. Schedule

In `First`: `advance_clock → apply_pending → net_deliver → pump`, with `net_deliver` `.after(TimeSystems)` and `run_if(in_state(EngineMode::Play))`. `net_deliver` takes the runtime and registry, polls the transport, enforces queue bounds, calls `deliver` with `Time<Real>` elapsed, applies the returned requests (allowlist, edit-mode and loopback checks first), and logs refusals. `pump` then runs the microtasks callbacks queued. Delivery ignores `ScriptClock`, so a paused game still reconnects.

### 4. Generations

JS keeps an ambient generation: the loading one during module evaluation; the live one in `runSystem`/`callEntry`; the owner's in a timer or socket callback. Every timer and socket records it, and every request carries it.

Both sides apply the same rule with no extra message: on `commit(n)` JS drops timers and socket objects of generations other than `n`, and core closes their connections with 1001; on `discard(n)` both drop only `n`'s; on `restart` both drop everything (core through a `RestartHandlers` entry). A compile failure never reaches `load`, so nothing moves.

### 5. Edit mode

`OnEnter(EngineMode::Edit)` closes every connection with 1001 and queues the `close` events in the registry; they are delivered at the first delivery in play. While in edit mode, an open request (which can only come from a load, since no system runs) is refused like a disallowed URL. Timers are not delivered in edit mode, since `net_deliver` does not run; due ones fire at the first frame of play.

### 6. Real transport: `async-tungstenite` 0.33 on `IoTaskPool`

One task per connection on `IoTaskPool`: DNS through `blocking::unblock(to_socket_addrs)`, `async_io::Async<TcpStream>`, `async_tungstenite::client_async_with_config` with `max_message_size` and `max_frame_size` 16 MiB. The task reads into a bounded `async_channel` and writes from an unbounded one. No `Origin` header is added; the request is built from the URL, the protocols and the allowed headers only. This is `bevy_remote`'s pattern (`async_io` reactor, channel back to the world). 0.33 reuses the locked `tungstenite` 0.28; 0.34+ would duplicate it.

Alternatives: `tokio-tungstenite` needs a turning tokio reactor; a thread per socket with sync `tungstenite` is not Bevy's pattern; our own glue over `tungstenite` and `Async<TcpStream>` would rewrite `async-tungstenite`.

### 7. Limits

Inbound: the channel holds 4,096 messages; the registry also counts queued bytes against 64 MiB. A message over 16 MiB (tungstenite's error) or a full queue closes the connection with 1009 after the queued messages are delivered, with `error` then `close`, and a log line naming the bound. Nothing is dropped silently. Outbound: unbounded, counted in `bufferedAmount` (JS adds on send, the I/O side reports bytes written in `NetEvent::Written`).

### 8. Policy

`bevy-ts.json`:

```jsonc
{ "net": { "allow": ["127.0.0.1:4477", "${ABYME_URL}"] }, "env": ["ABYME_URL", "ABYME_KEY"] }
```

`ProjectConfig` gains `net: NetConfig { allow: Vec<String> }` and `env: Vec<String>`. `GameFile` gains both with `#[serde(default)]`, so an older `game.json` reads as empty. One function, `NetPolicy::from_config(allow, env, &dyn Fn(&str) -> Option<String>)`, expands `${NAME}`, parses entries (host, host:port, or URL), and snapshots the visible env; dev and play both call it and insert the resource. Checks order: edit mode, scheme (`wss:` refused), allowlist on the URL's host before DNS, `headers` on non-loopback or naming `Origin`.

### 9. The globals in JS

A new `crates/bevy-ts-v8/js/web.js`, loaded before `core.js`'s console setup, defines `EventTarget`, `Event`, `CloseEvent`, `MessageEvent`, `DOMException`, `AbortController`/`AbortSignal`, `URL`/`URLSearchParams` (WHATWG URL parsing for `http(s)`/`ws(s)`/`file` URLs; enough for `connect.ts`, with tests from the WPT URL cases for those schemes), timers (a binary min-heap keyed by deadline then sequence), `WebSocket` and `process`. They live on `globalThis`, not on `bevy-ts`'s module, so packages see them.

`URL` written in JS rather than an op over the `url` crate: one call per parse would cross the seam; JS keeps it local and the backend free of a new op. If the WPT subset shows gaps, an op over `url` (already in the lock through `bevy_asset`) is the fallback.

### 10. `bevy-ts/globals`

`packages/bevy-ts/globals.d.ts`, exported as `bevy-ts/globals`. Classes are declared as `interface X` plus `declare var X: { prototype: X; new(...): X }`, the shape `lib.dom.d.ts` uses, so they merge with `@types/node`'s. `process` is `declare var process: NodeJS.Process` with `NodeJS.Process { env: NodeJS.ProcessEnv }` and `ProcessEnv { [key: string]: string | undefined }`, identical to `@types/node`'s, so the two merge. A test project with `@types/node` in `node_modules` type-checks it. Examples' `tsconfig.json` add `"types": ["bevy-ts/globals"]` and `"lib": ["es2023"]`.

### 11. Seam with script-packages

They own resolution; dynamic `import()` outside the compiled graph rejects catchably, so `typedAgainst` yields `null`. We own every web global, `process`, `bevy-ts/globals` and the `bevy-ts.json`/`game.json` reshape. One reload path. Our docs page is `docs/networking.md`; theirs is `docs/scripts.md`. Neither change edits the other's requirements.

## Risks / Trade-offs

- [Plain `ws:` exposes traffic on a network] → loopback is the only target now; headers refused elsewhere; `wss:` is the named follow-up.
- [A send waits up to one frame] → acceptable for a game; noted in docs.
- [Hand-written `URL` diverges from WHATWG] → WPT subset in tests; `url`-crate op as fallback.
- [`net`/`env` edits need an engine restart under `dev`] → documented; the studio's `restart` does it.
- [Edit mode closes sockets, so every Play reconnects] → the operator's ruling; `connect()`'s reconnect handles it.
- [A large burst parses on the main thread in one frame] → bounded by 4,096 messages and 64 MiB a connection; measured in a bench task.
- [`@types/node` versions change their global declarations] → the merge test pins the current one; re-check on upgrade.

## Migration Plan

Additive. A project without `net`/`env` gets no network and an empty `process.env`; a `game.json` from before reads the same. No protocol version change. Rollback is a revert.

## Open Questions

None blocking. Hot-applying `net`/`env` edits under `dev` can follow if the restart proves annoying.
