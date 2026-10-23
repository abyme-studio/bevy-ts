# script-networking: research

Findings for the change that gives game scripts `WebSocket` and timers. Research only; spec owns the change. Written 2026-10-23 by research@script-networking.

## 0. What the target client actually needs

The design says scripts need `WebSocket` and timers. Reading abyme's `web/packages/client/src/connect.ts`, `connect()` also needs:

| Global | Where | Without it |
|---|---|---|
| `process.env` (`ABYME_URL`, `ABYME_KEY`) | `connect.ts:543`, `:567` | falls back to `http://127.0.0.1:4477`, no key (works on the same Mac) |
| `URL` | `streamUrl`, `connect.ts:505` | `ReferenceError` on every `connect()` |
| `AbortController`, `AbortSignal` (`instanceof`, `addEventListener("abort")`, `.reason`) | `connect.ts:113,179,350`, `kernel/src/signals.ts:39` | `ReferenceError` on every call |
| `new WebSocket(url, { headers })` (non-standard; Deno 2.5+, Node's undici, Bun) | `connect.ts:568`, only with `ABYME_KEY` | no remote abymed |
| `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval` | `connect.ts:214,304,691` | `ReferenceError` |
| `fetch` | `connect.ts:693`, only with `opts.held` | the game does not pass `held` |
| dynamic `import(url, { with: { type: "json" } })` | `typedAgainst`, `connect.ts:519-531` | wrapped in `try`: yields `null`, contract version checks are skipped |

The engine today installs only `console` (`crates/bevy-ts-v8/js/core.js:81`). deno_core 0.412 ships no timers, `URL` or `AbortController` (those live in Deno's `deno_web`/`deno_url`, which the design rules out).

Recommendation: the change ships `WebSocket`, the four timer functions, `URL` (and `URLSearchParams`), `AbortController`/`AbortSignal`, a minimal `EventTarget`/`Event`/`CloseEvent`/`MessageEvent`/`DOMException`, and `process.env`/`process.argv`. `fetch` stays later. Sure: high (read the code).

## 1. Ops, pump, generations; where the new ops sit

Today:
- Ops: three, in `deno_core::extension!` (`crates/bevy-ts-v8/src/lib.rs:88-96`): `op_bevyts_log`, `op_bevyts_host`, `op_bevyts_host_error`. Host calls work only while a system runs (`lib.rs:67-69`), so a callback outside a system can change JS state but not the world.
- Pump: `First`, chained `advance_clock → apply_pending → pump` (`crates/bevy-ts/src/lib.rs:142-145`, `runtime.rs:187-197`). `pump` polls the event loop once with a no-op waker (`bevy-ts-v8/src/lib.rs:566-576`): it runs microtasks and the inspector, nothing else. `pump` is not ordered after `TimeSystems`.
- Generations: `apply` loads, then commits or discards (`crates/bevy-ts/src/generation.rs:139-250`, `load_and_apply` at `:268`). A failed load keeps the old generation live. A `restart` reload or a JS heap over 1 GiB makes a fresh isolate (`generation.rs:165-174`); plugins hear that through `RestartHandlers` (`plugin_api.rs:58-61,151`). The backend seam is `ScriptBackend` (`crates/bevy-ts/src/backend.rs:264-308`); its `pump` doc already says "microtasks, timers", but nothing implements timers.
- Timers or an event loop: none. `grep setTimeout` finds nothing in the runtime.

Recommendation:
- Rust side, backend-agnostic, in `bevy-ts` core: a `NonSend` connection registry (as the design says), the allowlist check, and a `Transport` seam with two impls, real and fake. Each connection and timer is tagged with the generation that made it: the loading one during module evaluation, else the live one.
- V8 side: ops `op_bevyts_ws_open(url, protocols, headers) -> id`, `op_bevyts_ws_send(id, text | bytes)`, `op_bevyts_ws_close(id, code, reason)`. Events come back in one batch per frame through one new host function on `__bevyts` (next to `runSystem`, `lib.rs:361-369`), never one JS call per message. Zero calls when nothing is pending.
- Timers in JS (core.js keeps a min-heap), driven from Rust: Rust passes `now` in the per-frame delivery call; JS fires what is due and returns its next deadline, so Rust skips the call until then.
- Order in `First`: `apply_pending → deliver (socket events, then due timers) → pump`, with `deliver` `.after(TimeSystems)`. `pump` then flushes the microtasks the callbacks queued.
- Generation scope: on commit of a new generation, close the previous generation's sockets (code 1001, "going away", what a browser sends when a page unloads) and drop its timers. On discard, do the same for the discarded generation only. On restart, close everything. A failed reload touches nothing.

Sure: high on today's code; medium on the op shape (spec may prefer one op per call or a batch op).

## 2. WebSocket client crate

In `Cargo.lock` today:

| Crate | Version | Note |
|---|---|---|
| `tungstenite` | 0.28.0 | sync; used by the inspector (`bevy-ts-v8/src/inspector.rs:98`) and a CLI test; no TLS feature |
| `async-io` | 2.6.0 | via `bevy_remote`, `bevy_asset`, `async-fs` |
| `async-channel` | 2.5.0 | |
| `blocking` | 1.7.0 | |
| `futures-io`, `futures-core`, `futures-task`, `atomic-waker`, `pin-project-lite`, `log` | present | |
| `tokio` | 1.53.3 | current-thread, `rt` only for bevy-ts (`Cargo.toml:17`); never turned |
| `async-tungstenite`, `tokio-tungstenite`, `async-net`, any `rustls`, `native-tls` | absent | |

Bevy's own precedent: `bevy_remote` serves HTTP on `IoTaskPool` with `async_io::Async<TcpStream>` and `async_channel` back to the world (`bevy_remote-0.20.0/src/http.rs:19-20,241,276,285`). `bevy_asset`'s web reader runs blocking I/O with `blocking::unblock` (`bevy_asset-0.20.0/src/io/web.rs:132,165`). `async-io` runs its own reactor thread, so it works under any executor, `IoTaskPool` included.

Pick: `async-tungstenite` **0.33** with default features off, over `async_io::Async<TcpStream>`, on `IoTaskPool`. DNS through `blocking::unblock(to_socket_addrs)`, as `bevy_asset` does. 0.33 depends on `tungstenite ^0.28`, the version already locked, and every other dependency of it is already locked (crates.io API, 2026-10-23). 0.34+ moves to tungstenite 0.29 and would duplicate it.

This is **one new crate** (the operator decides). The alternative with no new crate is our own glue: sync `tungstenite` over a non-blocking `Async<TcpStream>`, retrying on `WouldBlock`. That is what `async-tungstenite` already is; writing it ourselves is the workaround the foundations rule warns against.

TLS (`wss:`): no TLS crate is in the lock. Adding one (`futures-rustls` + `rustls` + a verifier, or `async-native-tls`) is a larger dependency and a crypto backend choice. Recommendation: v1 supports `ws:` only and refuses `wss:` with a clear error. That covers the plan (abymed on the same Mac, loopback, no key, `abymed.md:734-737`). `wss:` comes with the first remote abymed. Note the risk below (§6).

Not chosen: `tokio-tungstenite` (needs a turning tokio reactor, which is what the design avoids); Deno's `deno_websocket` (design ruled out); a dedicated thread per socket with sync `tungstenite` (one OS thread per socket, not Bevy's pattern).

Sure: high.

## 3. How timers tick

Recommendation: timers run on `Time<Real>`, and keep running while the game is paused or in edit mode.

- Web semantics: `setTimeout` is wall-clock in browsers, Deno and Node. The client's reconnect backoff (`connect.ts:302-304`) and keep-alive timers assume that; a paused editor would otherwise stall a reconnect past abymed's 15-second grace.
- Pause stops the bevy-ts stages and `Time<Virtual>` (`runtime.rs:158-180`, remote-protocol spec `bevy-ts/pause`). That is still safe: a timer callback cannot write the world, since host calls work only inside a system (`bevy-ts-v8/src/lib.rs:67-69`). Its effect waits for the next system run.
- Engines agree: Godot's `SceneTree.create_timer` defaults to `process_always = true` (runs while paused); Unity separates `WaitForSeconds` (scaled) from `WaitForSecondsRealtime`. Game-time delays in bevy-ts belong to systems and `Res<Time>`, not `setTimeout`.
- Tests stay deterministic: Bevy's `TimeUpdateStrategy::ManualDuration` drives `Time<Real>` (`bevy_time-0.20.0/src/lib.rs:185-186`).
- Granularity is one frame. No catch-up: after a long frame (a breakpoint, a hitch), an interval fires once and reschedules from now, as browsers do. Ties fire in deadline, then creation, order.
- `setTimeout` returns a number. `unref()` is not needed (`connect.ts:701` calls it optionally).

Sure: high on the facts; the paused behaviour is a choice (§6).

## 4. Where the project declares things; the API's shape

Project file today: `bevy-ts.json`, parsed in `crates/bevy-ts-cli/src/project.rs:60-75` (`name`, `entry`, `native`, `features`, `engineCache`, `levels`). The pattern for an engine setting: the CLI reads it and sets an engine resource before the run (`levels.prefabEdits`, `cli.rs:283-285`). A built game reads only `game/game.json`, written at build (spec@script-packages, §5), so the build must write these settings there.

Recommendation, in that pattern:

```jsonc
{
  "net": { "allow": ["127.0.0.1:4477", "${ABYME_URL}"] },  // host[:port], as Deno's --allow-net
  "env": ["ABYME_URL", "ABYME_KEY"]                        // the only variables process.env shows
}
```

- Allowlist entries are `host[:port]`, scheme-agnostic, as Deno's `--allow-net` (`ws:` and `http:` are the same origin for this). An entry may be `${NAME}`, read from the environment at start; an unset variable adds nothing. The check is on the URL's host before any DNS, so a refused URL never opens a socket. Refusal is the web's shape: the constructor succeeds, then `error` and `close` (code 1006) fire, and the log says which origin to add to `net.allow`.
- `process.env` shows only the names in `env`; `process.argv` is the arguments after `--` on `bevy-ts dev`/`play`. Deny by default, as the network.

WebSocket shape, from WHATWG and Deno:
- Events `open`, `message`, `error` (plain `Event`, no detail), `close` (`CloseEvent` with `code`, `reason`, `wasClean`); both `onX` and `addEventListener`.
- `readyState` 0-3 and the four constants, `url`, `protocol`, `extensions`, `bufferedAmount`.
- `close(code?, reason?)`: only 1000 or 3000-4999 (else `InvalidAccessError`), reason at most 123 UTF-8 bytes (else `SyntaxError`). A dropped connection reports 1006; a close frame without a code reports 1005.
- Binary: `binaryType` is `"blob"` by default on the web. Recommendation: support only `"arraybuffer"` and make it the default, since there is no `Blob`; a script setting `"blob"` gets a warning. Text frames arrive as strings.
- Second constructor argument: `protocols` (string or array), or Deno's options object `{ protocols, headers }` (Deno 2.5+, also Node and Bun). The client needs `headers` for `ABYME_KEY`.
- Bevy has no script networking; nothing to match there.

Sure: high on the shapes; medium on the file layout (spec may nest under one key).

## 5. Seams with script-packages

Asked lead@script-packages on 2026-10-23. Their answer (lead; spec@script-packages owns the seams): npm imports resolve at compile time with `oxc_resolver` and are rewritten to relative paths; ESM only; CJS and `node:` builtins refused; the watcher adds `package.json` and missing paths, so a `pnpm install` is a normal generation reload; no other reload path is planned.

What follows for us:
- Reloads: one path, the normal generation reload, so generation-scoped closing covers package changes too.
- `typedAgainst`'s computed dynamic `import()` cannot be rewritten at compile time; it throws inside its `try` and yields `null`. Harmless; contract checks are skipped in the engine.
- `node:` refused fits: we ship `process` as a global, not a `node:process` module.

Agreed with spec@script-packages (2026-10-23; it goes in their change, `openspec/changes/script-packages` on branch `script-packages`):
1. Packages owns module resolution. Dynamic `import()` resolves only against the compiled graph; a computed URL or a bare specifier outside it rejects, catchably. JSON modules and import attributes stay unsupported (they reject, catchably), so `connect.ts:519-531` is fine. `import.meta.url` is the module's real `file://` URL in dev and its shipped file under `game/scripts/node_modules/<name>@<version>/…` in a build.
2. Networking owns every web global and `process.env`/`process.argv`; packages adds none. Packages touches no tsconfig template, `@types` handling or `.d.ts`: `bevy-ts/globals` is ours.
3. Packages adds no `ProjectConfig` fields: the `bevy-ts.json` reshape is ours alone.
4. `bevy-ts build` ships one file per module under `game/scripts/`, no bundle. `bevy-ts play` reads only `game/game.json`, written at build, never `bevy-ts.json`. So `net.allow` and `env` must be baked into `game.json` at build; `${NAME}` entries stay unexpanded there and expand when the game starts.
5. No new reload path; a `pnpm install` is a normal generation reload.

Still ours to decide: how `bevy-ts/globals` reaches a project's types without clashing with an `@types/node` a package pulls into `node_modules`.

## 6. For the operator

1. New dependency: `async-tungstenite` 0.33, one crate (its dependencies are already locked). Alternative: our own glue over `tungstenite`. Recommend the crate.
2. TLS: `ws:` only in v1, `wss:` refused. Consequence: a remote abymed (bearer `ABYME_KEY`) is not reachable until TLS lands; sending the key over plain `ws:` to another machine would expose it, so the engine should refuse a `headers` option on a non-loopback `ws:` URL. Adding TLS later means choosing `rustls` (and a crypto backend) or the platform's TLS.
3. Timers on real time, running while paused. Alternative: virtual time, so a paused editor freezes scripts' timers too; that breaks reconnect backoff.
4. Deny by default for both network (`net.allow`) and environment (`env`). Alternative: show the whole environment, as Node does; any npm package in the game could then read every secret in the shell.
5. Edit mode: sockets and timers run in edit mode too (module evaluation runs there), so the town shows live data while edited. Alternative: hold the network until play.
6. Performance: socket I/O is off the main thread; the main-thread cost is one JS call per frame with pending events, plus the script's own parsing (a burst of records after each reconnect lands in one frame). Recommend a per-connection bounded queue and a maximum message size (16 MiB) so a flood cannot stall a frame without bound.

## Sources

- Code: file:line references above, this worktree at `0d2503a`; abyme at `~/dev/abyme` (main).
- [async-tungstenite on crates.io](https://crates.io/crates/async-tungstenite) (versions and dependencies via its API), [README 0.35](https://docs.rs/crate/async-tungstenite/0.35.0/source/README.md).
- [Deno `WebSocketOptions`](https://docs.deno.com/api/web/~/WebSocketOptions) (`headers`, non-standard), [Deno 2.5 release note](https://alternativeto.net/news/2025/9/deno-2-5-brings-permission-sets-websocket-header-support-new-runtime-api-and-more).
- WHATWG WebSockets standard (close codes, `binaryType`); Godot `SceneTree.create_timer`; Unity `WaitForSecondsRealtime` (from memory, not re-checked today).
