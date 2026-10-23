## Why

A game script cannot open a network connection or set a timer: the engine installs only `console`. Agent town step 2 needs the game to run `@abyme/client`'s `connect()` unchanged against abymed on the same Mac. `connect()` needs `WebSocket`, timers, `URL`, `AbortController` and `process.env` (research.md, §0). Bevy has no script networking, so this is a foundation, not a port.

## What Changes

- **Web globals for scripts.** The engine installs `WebSocket`, `setTimeout`/`clearTimeout`/`setInterval`/`clearInterval`, `URL`/`URLSearchParams`, `AbortController`/`AbortSignal`, a minimal `EventTarget`/`Event`/`CloseEvent`/`MessageEvent`/`DOMException`, and `process.env`. Each has the web's shape. `fetch` is not in this change.
- **`WebSocket` client, `ws:` only.** The web-standard `new WebSocket(url, protocols)`: text and binary frames, the web's events and close codes. `wss:` is refused with a clear error; it is a follow-up that needs a TLS crate the operator approves. A Deno-shaped `{ protocols, headers }` option is a marked extension nothing in bevy-ts depends on, refused on a non-loopback `ws:` URL so a key never crosses the network in clear text. The handshake sends no `Origin` header, as Deno does; abymed answers `403` to a foreign one.
- **Deny by default.** `bevy-ts.json` gains `net.allow` (hosts a script may connect to, as Deno's `--allow-net`) and `env` (the only variables `process.env` shows). A refused URL never opens a socket.
- **Shipped games keep the settings.** `bevy-ts build` writes `net` and `env` into `game/game.json`; `${NAME}` entries expand when the game starts.
- **Socket I/O off the main thread, one delivery a frame.** A `NonSend` registry in `bevy-ts` core owns connections behind a `Transport` seam (real and fake). Socket events and due timers reach JS in one call in `First`, after time advances, and none when nothing is due.
- **Timers on real time.** They keep running while the game is paused, as the web's do.
- **Networking follows what runs scripts.** Edit mode runs no game code, so it has no connections and fires no timers: an open in edit mode is refused, sockets close (1001) when the game leaves play, and timers wait for play.
- **Connections and timers belong to a generation.** Committing a generation closes the previous one's sockets (1001) and drops its timers; a discarded or failed load closes only its own; a restart closes all.
- **Bounded.** A message over 16 MiB, or a per-connection inbound queue over its bound, closes the connection with 1009 and fires `error` and `close`. No message is dropped silently.
- **Types.** `bevy-ts/globals` declares these globals for a project's `tsconfig.json`, and merges with an `@types/node` a package pulls in.
- **No protocol change.** Refusals and errors go through the existing log.

## Capabilities

### New Capabilities

- `script-networking`: the web globals scripts get, the `WebSocket` client, timers, generation scope, `net.allow` and `env` in `bevy-ts.json` and `game.json`, limits, the fake transport, and `bevy-ts/globals`.

### Modified Capabilities

- `script-runtime`: "Engine-neutral seam" gains the per-frame network and timer delivery and the visible environment on a load.
- `edit-mode`: "Edit mode runs no game code" adds timers and sockets: none fire in edit mode, and leaving play closes the connections.

## Impact

- **Decided for agent town (lead@script-networking, 2026-10-23, under the operator's overnight go):**
  1. New dependency: `async-tungstenite` 0.33 (default features off) in `bevy-ts` core. Its dependencies are already in `Cargo.lock`; it reuses the locked `tungstenite` 0.28.
  2. `ws:` only. `wss:`, and so a remote abymed, is a follow-up needing a TLS crate the operator approves.
  3. Web-standard constructor; `headers` only as a marked Deno-shaped extension, refused on non-loopback `ws:`.
  4. Timers on `Time<Real>`, running while paused.
  5. Deny by default for network and environment.
  6. No networking or timers in edit mode; sockets close when the game leaves play.
  7. Performance: a bounded inbound queue per connection and a 16 MiB message cap, both closing with 1009; one JS call a frame at most, none when idle.
- Code: `crates/bevy-ts/src/net/` (new: registry, policy, `Transport`, real and fake transports, delivery system), `crates/bevy-ts/src/backend.rs` (seam), `generation.rs` (close by generation), `lib.rs` (system order in `First`); `crates/bevy-ts-v8/src/lib.rs` (ops, delivery call), `crates/bevy-ts-v8/js/web.js` (new: the globals), `core.js` (generation tags); `crates/bevy-ts-cli/src/project.rs`, `cli.rs`, `play.rs`, `build.rs` (settings); `packages/bevy-ts/globals.d.ts` (new) and `package.json` exports; example `tsconfig.json`s.
- Seam with `script-packages` (agreed 2026-10-23): they own module resolution and add no globals, no `ProjectConfig` field and no types; one reload path, the generation reload.
- Docs: `docs/networking.md` (new), `docs/build.md` (`game.json` gains `net` and `env`), README feature list.
- Tests: headless, through the fake transport, with `TimeUpdateStrategy::ManualDuration`; one loopback test against a local `tungstenite` server.
