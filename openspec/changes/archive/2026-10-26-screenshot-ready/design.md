## Context

`crates/bevy-ts-remote/src/screenshot.rs` owns every shot (`capture`, the method, results). `BEVY_TS_SCREENSHOT` is `headless::screenshot_on_request` in the cli: a `Local` frame counter that fires at app frame 60. Bevy's `PipelineCache` lives in the render world. The vendored `bevy_render` compiles pipelines in the background on macOS, so a pipeline can be waiting for many frames. A pipeline whose shader is missing stays waiting for ever (`ShaderNotLoaded` retries). `TimeUpdateStrategy` is read in `First` by `TimeSystems`.

## Decisions

1. **Ready = script started and pipelines idle.**
   - "Script started": `RuntimeInfo.generation > 0`, or no `RuntimeInfo` (an engine without bevy-ts).
   - "Pipelines idle": a render-world system in `RenderSystems::Cleanup` stores `cache.waiting_pipelines().next().is_none()` into an `Arc<AtomicBool>` shared with the main-world resource `PipelinesIdle`. This uses Bevy's own state, not a counter of ours; `pipeline_trace` reads the same thing.
   - The render world runs behind, by one frame or two: pipelined, the render of the last frame may still be running. Reading whichever render finished last makes that lag vary with load, and the frames counted ready just after a spawn, before its new pipelines show as waiting, vary with it. So each render stores its waiting count by the `FrameCount` it extracted, and the main world reads the render of the frame before, which has always finished. The lag is fixed, so the ready frames, and the time they step, are the same every run.
   - "No asset load in flight" (change item `assets-ready`): Bevy lists no loads in flight, so `loads` walks every reflected component for `Handle`s (`ReflectHandle`), skipping types whose `TypeInfo` cannot hold one, and reads each handle's `RecursiveDependencyLoadState`. A handle the server does not manage (`Assets::add`, a script's material) is walked into through `ReflectAsset`, so its textures count. A `WorldAssetRoot` or `DynamicWorldRoot` counts until `WorldInstanceSpawner::instance_is_ready`. A failed asset does not wait; the shot warns naming it. The walk runs only while a shot is due or `FixedStep` is on.
2. **`Ready { frames }`**, updated in `First` before `TimeSystems`: +1 on a ready frame, 0 otherwise. The env shot fires when it reaches 60, once. A later material that queues a pipeline restarts the count.
3. **The method waits for idle pipelines and loads, not for the script.** A client driving a dev engine may want a shot of an engine whose script failed. A new `Progress::Waiting { since }` holds until pipelines are idle and no load is in flight, then captures as before. If either is still busy after `READY_TIMEOUT` (60 s), it fails naming the path and what is waiting.
4. **Fixed step.** A `FixedStep` resource is inserted when `BEVY_TS_SCREENSHOT` or `BEVY_TS_FIXED_STEP=1` is set. The `First` system sets `TimeUpdateStrategy::ManualDuration(1/60 s)` on a ready frame and `ManualDuration(ZERO)` otherwise. So game time is 1/60 s times the ready frames, whatever the load and however long compiles take. Systems still run while not ready, with `dt = 0`. A script counting frames rather than time would see the extra frames; that is documented, not hidden.
5. **Give up after 60 s.** If the env shot is not ready 60 s of wall time after start, it shoots anyway and warns with the waiting count. A run always produces its image, and the log says why it may be wrong.
6. **`screenshot_on_request` moves into `screenshot.rs`** as `shoot_when_ready`, driven by a `ShotWhenReady { path }` resource. The cli inserts that from the env; tests insert it directly, with no process-wide env in parallel tests. One owner for shots.
7. **`BEVY_TS_PIPELINE_TRACE`** is already in `base_app`, so every engine carries it. Shipping it on npm is the next release. No code change.

## Risks

- The asset walk reflects every component that can hold a handle, each frame a shot is due or `FixedStep` is on. Only screenshot and fixed-step runs pay for it.
- A game that queues pipelines, or starts a load, every frame would never be ready. The 60 s give-up covers the env shot, and the method fails saying so.
- `ManualDuration(ZERO)` while not ready also holds `Time<Fixed>`, so a fixed-update game does not step until ready. That is what a deterministic shot wants.
