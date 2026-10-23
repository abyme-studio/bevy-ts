## 1. Contract

- [x] 1.1 Add `bevy-ts/screenshot` (`x-bevy-ts-crate: bevy-ts-remote`, no `x-bevy-ts-watch`), `ScreenshotParams { path }` and `ScreenshotResult { path, width, height, frame }` to `protocol/bevy-ts.openrpc.json`; bump `info.version` to the next free minor (1.27.0 today)
- [x] 1.2 Regenerate with `scripts/gen-protocol.sh` (cargo through `~/dev/abyme/scripts/gate-slot`); commit `protocol.rs` and `packages/bevy-ts/protocol.d.ts`

## 2. Deferred methods

- [x] 2.1 `add_deferred_method` in `crates/bevy-ts-remote/src/lib.rs`: contract checked with `watch: false`, registered as `RemoteMethodSystemId::Watching`; doc comment cites Bevy's `+watch` check in `http.rs`
- [x] 2.2 The `remote-protocol` "Served on top of stock bevy_remote" docs in lib.rs name the third kind

## 3. The owner

- [x] 3.1 `crates/bevy-ts-remote/src/screenshot.rs`: `ScreenshotTarget(RenderTarget)` resource, `capture(commands, path) -> Entity` spawning Bevy's `Screenshot` with `save_to_disk(path)`, a size-recording `ScreenshotCaptured` observer and a `Shot` marker
- [x] 3.2 The `bevy-ts/screenshot` handler: validate (absolute, `.png`, parent is a directory) before capture; pending shots keyed by params; answer on `Captured` with the file newer than asked, else the "did not write" error; 5 s wall-time timeout despawns and errors
- [x] 3.3 Register it in `add_methods`

## 4. Callers

- [x] 4.1 `HeadlessPlugin` inserts `ScreenshotTarget(RenderTarget::Image(offscreen))`
- [x] 4.2 `screenshot_on_request` calls `capture` once at frame 60; frame 75 and `.2.png` removed; doc comment and module doc updated
- [x] 4.3 `crates/bevy-ts-viewport`: the ring's render-world code reads back the slot of a frame whose virtual or viewport window has a `Capturing` `Screenshot`, and the main world inserts `Captured` and triggers `ScreenshotCaptured` on it; mark it a workaround for Bevy's `SurfaceData`-only window screenshots, to drop when Bevy fixes it

## 5. Tests

- [x] 5.1 `crates/bevy-ts-cli/tests/screenshot.rs`, headless: clear colour changes once the game has run 30 frames; pause, step to 29 then 30, call the method through `BrpSender`, update until it answers; the PNG exists, is 960x540, has the colour of that frame, and `frame` is the update it rendered in (bevy-ts's frame counts paused updates too)
- [x] 5.2 Same file: a missing folder, a relative path and a `.jpg` fail naming the path, and spawn no `Screenshot`
- [x] 5.3 Same file: over `RemoteHttpPlugin` on a free port, the reply is one plain JSON-RPC response
- [x] 5.4 Viewport: a shot of the virtual window with a ring attached, in the existing viewport surface test (`crates/bevy-ts-cli/tests/surface.rs`) or beside it; macOS only
- [x] 5.5 `BEVY_TS_SCREENSHOT` writes `<path>` once and no `.2.png`

## 6. Users and docs

- [x] 6.1 `examples/cube/glow-walk.mjs`: drop the variable; after step 2 call `bevy-ts/screenshot` to `target/glow-walk.png` and note PASS/FAIL on its answer; header comment updated
- [x] 6.2 `.abyme/processes.ts`: `ui-walk` and `arena-shot` drop `.png*` globs for the one file
- [x] 6.3 `crates/bevy-ts-cli/tests/headless_ui.rs` doc comment
- [x] 6.4 `docs/build.md` headless paragraph: the method (chosen time via pause and step) and the variable (one shot, frame 60, for runs with no protocol)
- [x] 6.5 `docs/viewport.md`: one line that `bevy-ts/screenshot` captures the viewport's frame
- [ ] 6.6 Update `openspec/specs/headless-run/spec.md` Purpose at archive: "saved by `bevy-ts/screenshot` or `BEVY_TS_SCREENSHOT`"
