## 1. Ready

- [ ] 1.1 `screenshot.rs`: `PipelinesIdle(Arc<AtomicBool>)`, written by a render-world system in `RenderSystems::Cleanup` from `PipelineCache::waiting_pipelines`; `Ready { frames }`, counted in `First` before `TimeSystems`; `ReadyPlugin` adds both
- [ ] 1.2 `FixedStep`: the `First` system sets `TimeUpdateStrategy::ManualDuration` (1/60 s ready, zero otherwise)
- [ ] 1.3 `ShotWhenReady { path }` and `shoot_when_ready`: shoot at the 60th ready frame, or after 60 s with a warning; `headless::screenshot_on_request` removed
- [ ] 1.4 cli `base_app`: add `ReadyPlugin`; insert `ShotWhenReady` and `FixedStep` from `BEVY_TS_SCREENSHOT` and `BEVY_TS_FIXED_STEP`

## 2. Method

- [ ] 2.1 `Progress::Waiting { since }`: capture once pipelines are idle; fail after 60 s naming the path and the waiting count
- [ ] 2.2 Contract: the screenshot summary names the wait; protocol 1.30.0 → 1.31.0; regenerate

## 3. Tests

- [ ] 3.1 A script whose first generation applies 100 frames in: the shot shows its scene
- [ ] 3.2 A material compiled in the background: the shot at the first ready frame shows it
- [ ] 3.3 Two runs, one with 30 ms extra work a frame: identical pixels
- [ ] 3.4 The existing screenshot, headless_ui and viewport tests still pass

## 4. Docs

- [ ] 4.1 `docs/build.md` headless paragraph: ready frames, fixed step, `BEVY_TS_FIXED_STEP`
- [ ] 4.2 Note for the animation test's owner: `ManualDuration` in `anim_app` makes `animations_seek_rewind_replay` deterministic
