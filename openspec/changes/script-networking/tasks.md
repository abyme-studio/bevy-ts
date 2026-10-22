## 1. Policy and settings

- [x] 1.1 `crates/bevy-ts/src/net/policy.rs`: `NetPolicy::from_config(allow, env, lookup)`: `${NAME}` expansion, entry parsing (host, host:port, URL), visible-env snapshot; the check order (edit mode, `wss:`, allowlist before DNS, `headers` on non-loopback or naming `Origin`) with a refusal reason for the log. Unit tests for each entry form, case, `localhost` vs `127.0.0.1`, unset variables and every refusal.
- [x] 1.2 `crates/bevy-ts-cli/src/project.rs`: `ProjectConfig` gains `net.allow` and `env`; `cli.rs` inserts `NetPolicy` from them before the run.
- [x] 1.3 `play.rs`: `GameFile` gains `net` and `env` (`serde(default)`); `play::app` inserts `NetPolicy` from them. `build.rs` writes them unexpanded. Test: build round-trip keeps `${ABYME_URL}` as written; an older `game.json` reads as empty.

## 2. Core registry, transport seam, delivery

- [x] 2.1 `crates/bevy-ts/src/net/`: `ConnId`, `OpenRequest`, `Frame`, `NetRequest`, `NetEvent` (open, message, error, close, written), the `Transport` trait, and the `NonSend` `NetRegistry` (connections by id with generation, state, queued count and bytes).
- [x] 2.2 `FakeTransport` in core: accept or refuse an open, push text and binary, server close with a code, drop, read sent frames and handshake headers; no socket, no thread. Public for tests in other crates.
- [x] 2.3 `ScriptBackend::deliver(now_ms, events) -> Delivered { requests, next_timer_ms }`; `Generation` gains `env`; the fake backend implements both and records calls.
- [x] 2.4 `net_deliver` in `First`: `apply_pending → net_deliver → pump`, `.after(TimeSystems)`, `run_if(in_state(EngineMode::Play))`; polls the transport, enforces limits, calls `deliver` only with events or a due deadline, applies requests through `NetPolicy`, logs refusals.
- [x] 2.5 Limits: 4,096 messages and 64 MiB queued a connection; over either, or a message over 16 MiB, deliver what is queued, then `error` and `close` 1009, and log which bound. Tests with the fake transport for the burst (2,000), flood (5,000) and huge-message scenarios.
- [x] 2.6 Generations in `generation.rs`: drain load's requests tagged with the loading generation; on commit close the previous generation's connections with 1001; on discard close only the failed load's; a `RestartHandlers` entry and the heap-restart path close all. Tests: save while connected, failed reload while connected, restart.
- [x] 2.7 Edit mode: `OnEnter(EngineMode::Edit)` closes every connection with 1001 and queues `close` for the first delivery in play; opens in edit mode refused. Tests: `--edit` load opens nothing, play→edit→play delivers 1001, a `Startup` connect waits for the first Play, a timer set before edit fires at the first frame of play.

## 3. Real transport

- [x] 3.1 Add `async-tungstenite` 0.33 (default features off) to `crates/bevy-ts`; confirm `Cargo.lock` gains only that crate and keeps one `tungstenite` (0.28).
- [x] 3.2 `SocketTransport`: one `IoTaskPool` task per connection; DNS via `blocking::unblock`; `async_io::Async<TcpStream>`; `client_async_with_config` with 16 MiB message and frame caps; bounded inbound and unbounded outbound `async_channel`s; no `Origin` header; protocols and allowed headers only.
- [x] 3.3 Loopback tests against a local `tungstenite` server on an ephemeral port: echo of text and binary, server close codes (1000, 1005 without a code, 1006 on drop), protocol negotiation, the handshake carries no `Origin`, and a 20 MiB message closes with 1009.

## 4. V8 backend and the globals

- [x] 4.1 `crates/bevy-ts-v8/js/web.js`, loaded before modules: `EventTarget`, `Event`, `CloseEvent`, `MessageEvent`, `DOMException`, `AbortController`/`AbortSignal` (`abort`, `timeout`, `throwIfAborted`, `reason`), `URL`/`URLSearchParams`, `process` with a frozen `env`. Tests: the WPT URL cases for `http(s)`, `ws(s)` and `file`; `AbortSignal` scenarios; `typeof fetch === "undefined"`.
- [x] 4.2 Timers in `web.js`: min-heap by deadline then sequence; ids are positive integers; extra args; negative or missing delay is 0; string callback throws; intervals reschedule from firing time with no catch-up.
- [x] 4.3 `WebSocket` in `web.js`: constructor forms (protocols string or array; Deno-shaped `{ protocols, headers }` marked non-standard), `http:`→`ws:`, `readyState` constants, `bufferedAmount`, `binaryType` (`"blob"` warns), `send` rules, `close` code and reason checks, `onX` and `addEventListener`.
- [x] 4.4 Ops `op_bevyts_ws_open`, `op_bevyts_ws_send` (string or bytes), `op_bevyts_ws_close` appending to an `OpState` outbox; `load` and `deliver` drain it.
- [x] 4.5 `core.js`: the ambient generation (loading, live, or the callback owner's); `commit`/`discard`/`restart` drop other generations' timers and sockets; `__bevyts.deliver(now, batch)` dispatches events in order, fires due timers, returns the next deadline. Callbacks run outside any system, so a Rust param call throws the existing error.
- [x] 4.6 `deliver` in the V8 backend: no JS call when nothing is pending or due (test with `calls_into_js` over 100 idle frames); one call for 500 messages.

## 5. End-to-end, headless

- [x] 5.1 Headless engine tests through `FakeTransport` and `TimeUpdateStrategy::ManualDuration`: a script opens `ws://127.0.0.1:4477/api/stream`, the test accepts and pushes a hello, the script's handlers run and its first sent frame is read; `setTimeout(f, 50)` at 16 ms frames fires in the fourth frame; a paused game still fires timers and delivers.
- [x] 5.2 Every spec scenario in `specs/script-networking/spec.md`, `specs/script-runtime/spec.md` and `specs/edit-mode/spec.md` has a test or is named here as covered by one above.
  Coverage: globals, URL, AbortSignal, `fetch` undefined, timers, constructor/close/send rules: `bevy-ts-v8/tests/web.rs`. Echo, 1000/1005/1006, protocols, no `Origin` on the wire, 20 MiB → 1009: `bevy-ts/tests/socket.rs`. Refusals (host, `wss:`, `Origin` header, headers to non-loopback), limits (burst, flood, 64 MiB, huge), generations (save, failed reload, restart), edit mode (load refused, close 1001 on play, timers wait), paused delivery, idle: `bevy-ts/tests/net.rs` and `bevy-ts-v8/tests/net.rs` (each scenario end to end in V8 too). Policy entry forms and env: `net::policy` unit tests; built game: `bevy-ts-cli` `play::tests`. Types with `@types/node`: `pnpm check` (`test/types-node`). Not tested directly: a callback's Rust param call throwing (delivery never sets the host slot, which only `run_system`/`call_entry` set) and a TS `Startup` system connecting at the first Play (its requests take the same outbox-to-delivery path as an entry's, tested).
- [x] 5.3 With `script-packages` landed: a fixture game imports `@abyme/client/connect` from a workspace link and runs `connect()` against a test abymed on its own port and state folder (never the live one on 4477), headless. If no test abymed can run, say what is unverified.
  Done 2026-10-23 before script-packages landed, so the game imported `connect.ts` by relative path: against a test abymed (port 4593, `fixtures/config`, a throwaway state folder), headless, `connect()` ran unchanged, got the hello, called `connections.all()` and `services.all()`, and closed. Unverified: importing through a workspace link (symlinked paths fail to load; reported to script-packages, who own resolution), `ABYME_KEY` with `headers`, watches, `offer()`, reconnect after abymed restarts, and `held` (needs `fetch`, out of scope).
- [x] 5.4 Bench: 2,000 1 KiB messages in one frame; record the frame's main-thread cost in `bench/`.
  `bench/networking/results/burst.md`: about 2.7 ms of main thread over an idle frame (median of 8, release).

## 6. Types

- [x] 6.1 `packages/bevy-ts/globals.d.ts`, exported as `bevy-ts/globals` in `package.json`; `lib.dom`-shaped class declarations; `process` through `NodeJS.Process`/`NodeJS.ProcessEnv`; `headers` documented as non-standard.
- [x] 6.2 Examples' `tsconfig.json`: `"types": ["bevy-ts/globals"]`, `"lib": ["es2023"]`; `tsc --noEmit` passes for each.
- [x] 6.3 A type test project with `@types/node` in `node_modules` that uses `new WebSocket(url, { headers })` and `process.env.ABYME_URL`: `tsc --noEmit` passes.

## 7. Docs

- [x] 7.1 `docs/networking.md` (new): the globals and what is missing (`fetch`, `wss:`, `Blob`, `argv`); `net.allow` and `env` with the agent-town example; refusals and their log lines; real-time timers; play-only networking; generation scope on reload; limits; the one-frame send latency; the `headers` extension and no `Origin`; testing with `FakeTransport`.
- [x] 7.2 `docs/build.md`: `game.json` gains `net` and `env`, unexpanded, expanded at start.
- [x] 7.3 README feature list: one line for script networking.

## 8. Gate

- [ ] 8.1 `cargo test` for `bevy-ts`, `bevy-ts-v8` and `bevy-ts-cli`, the package tests, and `openspec validate script-networking --strict` all pass.
