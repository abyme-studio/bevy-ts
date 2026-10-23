## Why

An agent or tool driving a running engine cannot ask it for a picture. Today the only way is `BEVY_TS_SCREENSHOT`, which saves at frames 60 and 75 whatever the game shows then. A walk that wants the view after a click, a reload or a stepped frame has no way to get it.

## What Changes

- New BRP method `bevy-ts/screenshot { path }`. It spawns Bevy's `Screenshot` entity with `save_to_disk(path)` and answers once the PNG is written, with its size and the frame it shows, or fails saying why.
- It captures what the run shows: the offscreen image under `--headless`, the primary window in a window, the viewport's frame under `--viewport`.
- A "chosen time" is the existing protocol's: pause, step to the frame, then screenshot. Rendering runs while paused.
- `bevy-ts-remote` gets `add_deferred_method`: a method whose one answer comes on a later frame, served on Bevy's watching registration under a plain name (Bevy's HTTP transport sends one plain response for a name without `+watch`).
- **BREAKING** `BEVY_TS_SCREENSHOT=<path>` saves one shot, at frame 60, through the same capture code. The frame-75 `<path>.2.png` is gone; a later shot is the method's job. The variable stays because `bevy-ts play --headless` serves no protocol.
- `examples/cube/glow-walk.mjs` takes its shot with the method after its edits, not with the variable.
- Protocol 1.26.0 → 1.27.0 (a method added).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `remote-protocol`: adds `bevy-ts/screenshot` and the deferred-answer method kind.
- `headless-run`: `BEVY_TS_SCREENSHOT` saves one shot at frame 60 through the screenshot owner; the method captures headless at any time.

## Impact

- Code: new `crates/bevy-ts-remote/src/screenshot.rs` (the one owner: target choice, capture, result) and `add_deferred_method` in `lib.rs`; `crates/bevy-ts-cli/src/headless.rs` (`screenshot_on_request` becomes a thin caller; headless names its offscreen image as the target); `crates/bevy-ts-viewport` (completes a capture of the virtual window, which Bevy 0.20 skips: it has no wgpu surface).
- Protocol: `protocol/bevy-ts.openrpc.json` gains `bevy-ts/screenshot`, `ScreenshotParams`, `ScreenshotResult`; regenerated Rust and TS types.
- Tests: `crates/bevy-ts-cli/tests/screenshot.rs`, headless: capture at a chosen frame, refuse a bad path. A viewport test beside the viewport crate's existing ones.
- Users of the variable: `examples/cube/glow-walk.mjs`, `.abyme/processes.ts` (`arena-shot`, `ui-walk` drop `.2.png`), `crates/bevy-ts-cli/tests/headless_ui.rs` doc comment.
- Docs: `docs/build.md` (headless section), `docs/viewport.md` (one line), `docs/materials.md` unchanged.
- Overlap: `animation-control` has no protocol method yet. If both add methods, the second to land rebases and takes the next minor.
- Cost: none while no shot is asked for. One extra GPU readback per shot.
- Ask-first: none. No new dependency (Bevy's `screenshot` module and `bevy_remote` only), no prompt or tool change, no data or production effect. `add_deferred_method` is a small helper over Bevy's own watching registration, not a new architecture.
