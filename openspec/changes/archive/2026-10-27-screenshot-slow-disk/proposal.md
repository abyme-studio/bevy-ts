## Why

`screenshot_ready::the_method_waits_for_a_texture_on_a_slow_disk` is red on main under load, and
`a_looping_clip_on_a_slow_gltf_is_the_same_at_any_load` hangs with no watchdog firing.

- `bevy-ts/screenshot` runs in `RemoteLast`, after Bevy's `Last` has advanced `FrameCount`. It read
  the pipelines of the frame before that, which is the render still in flight on the render thread.
  Under load that render has not finished, so the method saw "no frame rendered", and once the
  request was 5 s old it failed. The bug has been there since the method waited for loads.
- With that fixed, it captured on the update a texture finished loading. The textured material's
  pipelines are only queued in the render after, so the shot was black.
- The ready tests' watchdog was a resource inside the `App`. Dropping the app waits for the render
  thread, after the watchdog was already dropped, so a stalled render hung the run at 0% CPU.

## What Changes

- `ReadyPlugin` notes `FrameCount` in `First`; `pipelines_waiting` reads the render before that
  frame from any schedule of the update.
- `bevy-ts/screenshot` captures at the third ready update in a row, not the first: the third reads
  the render of the first.
- The ready tests hold their watchdog outside the app, so it outlives the app's drop, with a
  300 s bound: the method may wait out its own 60 s ready and 60 s readback timeouts, and 120 s
  fired at load 40 on runs that answered. They wait on the answer with `common::until`, not a frame
  count.

## Capabilities

### Modified Capabilities

- `remote-protocol`: `bevy-ts/screenshot` captures at the third ready update in a row.

## Impact

`crates/bevy-ts-remote/src/screenshot.rs`, `crates/bevy-ts-cli/tests/it/screenshot_ready.rs`, the
method's summary in `protocol/bevy-ts.openrpc.json` (no new shape, no version), `docs/build.md`.
