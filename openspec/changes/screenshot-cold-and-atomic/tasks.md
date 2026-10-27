## 1. Engine (`crates/bevy-ts-remote/src/screenshot.rs`)

- [ ] 1.1 `PipelinesWaiting` records pipelines done (`pipelines().count() - waiting`) per render; `pipelines_done(world)` reads it for the same frame `pipelines_waiting` reads
- [ ] 1.2 `ReadyTimeout(pub Duration)` resource, init by `ReadyPlugin` at `READY_TIMEOUT`; `READY_TIMEOUT`'s doc says it is a no-progress window
- [ ] 1.3 A progress clock both shots share: `since`, pipelines done then, fewest loads since; reset on more done or a new low of loads. `Progress::Waiting` and `ShotWhenReady` use it in place of their start time
- [ ] 1.4 `shoot_when_ready`: no "shooting anyway"; on no progress for `ReadyTimeout`, `error!` naming the path and `not_ready` (or "the script has not started"), no capture, `AppExit::from_code(1)`
- [ ] 1.5 `ShotWhenReady` keeps its `Shot` entity after `capture` and checks it with `result` each update: `Ok` removes the resource, `Err` fails as in 1.4 with the reason
- [ ] 1.6 `save_atomically(path)`: Bevy's `save_to_disk` to `.<stem>.tmp-<pid>-<n>.<ext>` in the target's folder (`.` for a bare name), then `rename`; on failure `error!` and remove the temp. `capture` observes it instead of `save_to_disk`
- [ ] 1.7 Module and item docs updated for 1.3-1.6

## 2. CLI

- [ ] 2.1 `crates/bevy-ts-cli/src/play.rs`: `run` returns the code of `app.run()`'s `AppExit`
- [ ] 2.2 `crates/bevy-ts-cli/src/cli.rs`: the dev run exits with `app.run()`'s code; `shot_from_env`'s doc says a shot that cannot be ready exits 1

## 3. Tests (docs/testing.md)

- [ ] 3.1 Unit, `screenshot.rs`: `a_reader_never_sees_a_partial_png`: an entity observing `save_atomically` is triggered with a large noisy `ScreenshotCaptured` several times while a thread reads the path in a loop; every read is absent or decodes whole; no temp file is left
- [ ] 3.2 Unit, `screenshot.rs`: the progress clock: with `ReadyTimeout` short, pipelines done rising every half window keeps a `ShotWhenReady` waiting past the window; no change for the window fails it with no file, an `AppExit` of code 1 and no capture (fed through `PipelinesWaiting`, as `the_shot_clock_skips_a_ready_frame_before_a_wait` does)
- [ ] 3.3 `crates/bevy-ts-cli/tests/it/screenshot_ready.rs`: `a_cold_build_longer_than_the_limit_is_waited_out`: six materials, each its own `Material` type with a fragment shader on `slow://`, each shader read released only after the last one's pipeline is done and a wait under the window; `ReadyTimeout` 3 s, total over 3 s; the env shot is byte-identical to the same scene with no read held, under `FixedStep`
- [ ] 3.4 `screenshot_ready.rs`: `a_pipeline_that_never_compiles_fails_the_env_shot`: one shader read held for ever, `ReadyTimeout` 1 s: no file, an `AppExit` of code 1; and `bevy-ts/screenshot` in the same app fails naming the path and pipelines waiting
- [ ] 3.5 `crates/bevy-ts-cli/tests/it/screenshot.rs`: `a_dev_engine_takes_its_env_shot_once…` pipes the engine's stderr and prints it when it fails or the engine exits

## 4. Docs and spec

- [ ] 4.1 `docs/build.md`, Headless paragraph: the 60 s is with no progress (what progress is); the env shot fails with exit 1 and no file instead of shooting anyway, and bounds its capture; both shots write a temp file and rename it
- [ ] 4.2 `CHANGELOG.md` Unreleased: `BEVY_TS_SCREENSHOT` exits 1 with no file when it cannot be ready; `bevy-ts play`/`dev` exit with the game's `AppExit` code
- [ ] 4.3 `openspec validate screenshot-cold-and-atomic --strict` passes; archive syncs `headless-run` and `remote-protocol`
