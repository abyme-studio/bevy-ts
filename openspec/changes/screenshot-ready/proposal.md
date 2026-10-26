## Why

A headless shot is not the same picture twice. Agent town's goldens flake under load in three ways:

- `BEVY_TS_SCREENSHOT` fires at frame 60 before the script has started, so the image is black.
- It fires before a material's pipeline has compiled, so the planets show the fallback. macOS now compiles pipelines in the background, which makes this more common.
- Idle animations follow wall-clock time, so frame 60 shows a different pose at a different load.

## What Changes

- A run is ready to shoot when its script has run (its first generation applied) and Bevy's `PipelineCache` has nothing waiting. A render-world system reads the cache's own `waiting_pipelines()` each frame; there is no counter of ours.
- `BEVY_TS_SCREENSHOT` saves at the 60th ready frame in a row, not at app frame 60. A frame that is not ready starts the count again.
- `bevy-ts/screenshot` waits for an idle pipeline cache before it captures. It fails, naming the path, if the cache is still busy after 60 s.
- Fixed step: time advances exactly 1/60 s a frame through Bevy's `TimeUpdateStrategy::ManualDuration`, and not at all while the run is not ready. It is on under `BEVY_TS_SCREENSHOT`, and under `BEVY_TS_FIXED_STEP=1` for a run driven over the protocol (a test mode). So frame N after ready is the same picture at any load.
- If a run is not ready after 60 s, `BEVY_TS_SCREENSHOT` shoots anyway and logs a warning saying how many pipelines are still waiting. A missing shader keeps a pipeline waiting for ever, and the run should still produce its image.
- `BEVY_TS_PIPELINE_TRACE` stays as it is. It is already in every engine's `base_app`, so it reaches npm with the next release.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `headless-run`: `BEVY_TS_SCREENSHOT` counts ready frames and runs on a fixed step.
- `remote-protocol`: `bevy-ts/screenshot` waits for idle pipelines; `BEVY_TS_FIXED_STEP`.

## Impact

- Code: `crates/bevy-ts-remote/src/screenshot.rs` keeps ownership of shots. It gains the ready count, the render-world idle reading, the fixed step, and the env shot's trigger. `crates/bevy-ts-cli/src/headless.rs` `screenshot_on_request` moves into it. `cli.rs` adds the plugin.
- Protocol: 1.30.0 → 1.31.0, the minor `bevy-ts/screenshot` should have taken. It landed on main inside 1.30.0: land's rebase kept reflect-assets' 1.30.0 over this method's 1.31.0. Its summary gains the pipeline wait. There are no type changes.
- Tests:
  - A slow-starting script, where the shot shows its scene, not black.
  - A late-compiling material, where the shot shows it, not the fallback.
  - Two runs at different loads, which give identical pixels.
- Docs: `docs/build.md` headless paragraph.
- Not changed: `animation::animations_seek_rewind_replay` builds its own app with no fixed step, so this change does not make it deterministic by itself. Inserting `TimeUpdateStrategy::ManualDuration` in that test would. That is noted for its owner.
- Cost: under fixed step, a game's time runs at 1/60 s per frame, not wall time. That is wanted for shots and only on under the two variables. Off them, nothing changes but one atomic store per render frame.
- Ask-first: none. Bevy's own `TimeUpdateStrategy` and `PipelineCache`, no dependency, no prompt or data change.
