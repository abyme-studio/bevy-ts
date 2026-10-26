## 1. Ready

- [x] 1.1 `screenshot.rs`: `PipelinesWaiting`, each render's `PipelineCache::waiting_pipelines` count by its `FrameCount`, written in `RenderSystems::Cleanup` and read for the frame before; `Ready { frames }`, counted in `First` before `TimeSystems`; `ReadyPlugin` adds both
- [x] 1.2 `FixedStep`: the `First` system sets `TimeUpdateStrategy::ManualDuration` (1/60 s ready, zero otherwise)
- [x] 1.3 `ShotWhenReady { path }` and `shoot_when_ready`: shoot at the 60th ready frame, or after 60 s with a warning; `headless::screenshot_on_request` removed
- [x] 1.5 `loads`: handles in reflected components (and in assets added without the server) at `RecursiveDependencyLoadState::Loading`, and world asset roots whose instance is not ready, keep a frame from being ready; failed assets are warned by path
- [x] 1.4 cli `base_app`: add `ReadyPlugin`; insert `ShotWhenReady` and `FixedStep` from `BEVY_TS_SCREENSHOT` and `BEVY_TS_FIXED_STEP`

## 2. Method

- [x] 2.1 `Progress::Waiting { since }`: capture once pipelines are idle and nothing loads; fail after 60 s naming the path and what waits
- [x] 2.2 Contract: the screenshot summary names the wait; protocol 1.30.0 → 1.31.0; regenerate

## 3. Tests

- [x] 3.1 A script whose first generation applies 100 frames in: the shot shows its scene
- [x] 3.2 A material compiled in the background: the shot at the first ready frame shows it
- [x] 3.3 Two runs, one with 30 ms extra work a frame: identical pixels
- [x] 3.5 A texture behind a held reader: no shot until it opens, then the texture
- [x] 3.6 Fox.glb from a slow reader, its run clip looping, on the fixed step: identical pixels at two loads
- [x] 3.4 The existing screenshot, headless_ui and viewport tests still pass

## 4. Docs

- [x] 4.1 `docs/build.md` headless paragraph: ready frames, fixed step, `BEVY_TS_FIXED_STEP`
- [x] 4.2 Note for the animation test's owner: `ManualDuration` in `anim_app` makes `animations_seek_rewind_replay` deterministic
