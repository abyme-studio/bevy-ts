## Why

When the GPU driver stops taking an engine's work, the render thread blocks, the main thread blocks in `renderer_extract`, and Update never runs again (`research.md`). Every timeout we have runs inside Update, so nothing fires: a headless run, a `BEVY_TS_SCREENSHOT` run or a render test hangs silently for minutes or for good. The cause is outside bevy-ts; the hang is ours to bound.

## What Changes

- **A stall watchdog.** A thread outside the main and render threads watches when each frame's main part and render part start and end. When one has been open for the limit (30 s of wall time), it names the stall on stderr: "Update stopped", the frame, when it started, and where it is stuck (in the frame's systems, waiting for the render world, or in the render).
- **What it does then.** Under `--headless` (which `BEVY_TS_SCREENSHOT` and the land check use) and in tests, it exits the process with code 75. With a window or `--viewport` it warns, and says so again when frames come back: a hidden window can stop frames on purpose. `--inspect` turns it off: a breakpoint stops Update on purpose.
- **The limit.** `BEVY_TS_STALL_SECS=<seconds>` sets it for an engine; `0` turns the watchdog off.
- **Open protocol requests are answered.** When it fires, every protocol request still open (a `bevy-ts/screenshot` waiting for frames, or a request no frame has read yet) gets a JSON-RPC error naming the stall, before the process exits. A client no longer waits on a socket that never answers.
- **Tests.** `common::render_app()` and every app built from `base_app` carry the watchdog, so every render test is bounded without a line of its own. **BREAKING (tests only):** `common::watchdog` and `screenshot_ready`'s `Hang` are removed; their callers rely on the engine's watchdog.

## Capabilities

### New Capabilities

- `stall-watchdog`: when a frame counts as stalled, what the watchdog says, what it does in each kind of run, its limit, and how tests use it.

### Modified Capabilities

- `remote-protocol`: an open request is answered with an error naming the stall when Update stops, instead of never.

## Impact

- Operator's go: none needed. No new dependency, no new service, no cost per frame beyond a few atomic stores. It changes no AI prompt or harness.
- Code: a new `crates/bevy-ts-remote/src/stall.rs` (the one owner); `crates/bevy-ts-cli/src/cli.rs` (`base_app` adds it, reads `BEVY_TS_STALL_SECS`, `--inspect` turns it off); `crates/bevy-ts-cli/tests/it/common/mod.rs` (`render_app` adds it, `watchdog` removed); `crates/bevy-ts-cli/tests/it/{screenshot_ready,ui_widgets,ui_placement}.rs` (calls removed); a new test module `crates/bevy-ts-cli/tests/it/stall.rs`.
- Protocol: no schema change. The error is `-32603` (internal error) with a message that starts `Update stopped`.
- Docs: `docs/build.md` (`BEVY_TS_STALL_SECS` beside `BEVY_TS_SCREENSHOT`), `docs/native.md` (the env list), `docs/testing.md` (render tests are bounded by the engine; no per-test watchdog).
- Neighbour: `ts-heap-fields-2`'s f39e9268b adds a `common::watchdog` call to `ambient_light.rs`. Whichever branch lands second drops that call; the engine's watchdog covers it.
- Not in this change: counting headless renders as heavy runs in gate-slot, and finding the GPU page faults (`research.md`, fixes 2 and 3).
