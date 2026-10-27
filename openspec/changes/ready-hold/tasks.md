## 1. Engine

- [ ] 1.1 `crates/bevy-ts-remote/src/screenshot.rs`: `ReadyHold(BTreeSet<String>)` with `hold`, `release`, `is_held`, `names`; `ReadyPlugin` inits it; module doc names it
- [ ] 1.2 Same file: `count_ready`'s `now` and the method's ready test are false while held; `not_ready` puts `held by the script: <names>` first; `shoot_when_ready` keeps "the script has not started" for generation 0
- [ ] 1.3 `crates/bevy-ts-remote/src/stall.rs`: after the `eprintln!`, `error!` the same line; module doc says so
- [ ] 1.4 New `crates/bevy-ts-cli/src/ready.rs` (`ScriptReadyPlugin`, `ReadyParam`, `#[export(ts = "ReadyHold", module = "bevy-ts/ready")]`: `hold`, `release`, `is_held`), `crates/bevy-ts-cli/ts/ready.{js,d.ts}` (module doc: hold in `Startup`, release where the data is applied, the 60 s failure), `pub mod ready` in `crates/bevy-ts-cli/src/lib.rs`, the plugin in `crates/bevy-ts-cli/src/cli.rs` beside `ScriptSessionPlugin`

## 2. Tests (docs/testing.md)

- [ ] 2.1 Unit, `screenshot.rs`: under `FixedStep`, 20 held frames with nothing pending give `Ready.frames == 0`, `steps == 0` and `ManualDuration(ZERO)`; the first frame after `release` steps `STEP`
- [ ] 2.2 Unit, `screenshot.rs`: a `ShotWhenReady` with a short `ReadyTimeout` and a hold never released gives up with a message containing `held by the script: abymed`; without `FixedStep` or a shot, `count_ready` leaves `TimeUpdateStrategy` alone
- [ ] 2.3 New `crates/bevy-ts-cli/tests/it/ready_hold.rs` (`mod ready_hold;` in `tests/it/main.rs`, macOS only like `screenshot_ready`, apps from `common::render_app()`): a TS script holds `"data"` in `Startup` and is first counted started on a held frame (no step before the hold)
- [ ] 2.4 Same module: the fox clip spawned on the release of a hold, the release after a wall delay (an `AtomicBool` a thread of the test sets after the delay: 0 ms; 400 ms with 30 ms of work a frame; the same again), shot at the 30th ready frame: three shots byte-identical; the same 0 ms and 400 ms arms without the hold differ (the test sees the bug)
- [ ] 2.5 Same module: `bevy-ts/screenshot` asked while held starts no capture until released; under a hold never released and a short `ReadyTimeout`, it fails naming the path and `held by the script: data`
- [ ] 2.6 `crates/bevy-ts-cli/tests/it/stall.rs`: an app with Bevy's log and a blocked system, its log captured, has an error record with `Update stopped:` (or a unit test on the logging call, if capturing Bevy's log in-process is not cheap)

## 3. Docs

- [ ] 3.1 `docs/build.md`: the hold beside the shot and `BEVY_TS_FIXED_STEP` (what it does, the TS example, the 60 s failure); the stall line is also logged as an error
- [ ] 3.2 `CHANGELOG.md`: `bevy-ts/ready`'s `ReadyHold`

## 4. Hand-off

- [ ] 4.1 Note to lead@engine-core for agent town: hold `"abymed"` from `connect()` until link applied its first records; count `LABELS_FROM` in steps; keep `Update stopped:` and exit lines in `scripts/shot-rounds.sh` and `frame.mjs`; recheck the 1 px leaf at (300,374) after
