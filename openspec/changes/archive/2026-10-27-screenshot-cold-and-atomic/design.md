## Context

`crates/bevy-ts-remote/src/screenshot.rs` owns both ready-frame shots. Today:

- `ShotWhenReady` (`BEVY_TS_SCREENSHOT`) starts a clock at its first update. After
  `READY_TIMEOUT` (60 s) it warns and calls `capture` anyway, then removes itself. Nothing watches
  the capture after that.
- `bevy-ts/screenshot` (`Progress::Waiting`) fails after 60 s from its request. Once captured,
  `result` bounds it: `TIMEOUT` (5 s) with no frame, `READBACK_TIMEOUT` (60 s) with no readback,
  and "captured but did not write".
- `capture` observes Bevy's `save_to_disk(path)`, which encodes and writes `path` in place,
  synchronously, inside the `ScreenshotCaptured` observer.
- `PipelinesWaiting` records, per render, the frame and how many pipelines wait. Bevy's
  `PipelineCache::pipelines()` counts every pipeline, waiting or not.
- `play::run` returns 0 and `cli.rs`'s dev run ignores `app.run()`'s `AppExit`.

## Goals / Non-Goals

**Goals:** no silent wrong shot; no silent missing shot; a cold build is waited out; a poller
never reads a partial PNG.

**Non-Goals:** a new env var or CLI flag for the limit; an overall cap on a build that keeps
progressing (the stall watchdog bounds a stopped Update); changing the 60-frame / 3-update ready
rule or the fixed step.

## Decisions

1. **The ready clock restarts on progress.** `PipelinesWaiting` also records how many pipelines
   are done (`pipelines().count() - waiting`). Each shot keeps `since` (last progress) and what it
   saw then: pipelines done, and the fewest loads in flight. A frame with more pipelines done, or
   fewer loads than that fewest, is progress: `since` resets. A shot fails when `since` is older
   than the window. The window is `ReadyTimeout(Duration)`, a resource `ReadyPlugin` inits at
   `READY_TIMEOUT` (60 s), read by both shots.
   - Why not a longer fixed limit: a cold build at load 30 takes no fixed time; a fixed limit
     either fails a slow good build or waits a minute for a hang.
   - Why the fewest loads, not any change: loads queued by a script go up and down; only a new
     low is work done, so a reload loop cannot keep a shot alive.
   - Why no env var: Bevy has none for this, and a clear failure tells the caller what to fix.
     Tests set `ReadyTimeout`, as stall tests set `StallSettings`.

2. **The env shot fails loudly.** When it cannot be ready, it logs `error!` with
   `<path>: not ready after <n> s with no progress (<why>); no shot saved` and writes
   `AppExit::from_code(1)`. `<why>` is `not_ready`'s list, or "the script has not started". Code
   1, not the watchdog's 75: a hang with Update running is a defect of the run (a missing shader),
   not a stall that may pass on retry.
   - `play::run` returns `app.run()`'s code and the dev run exits with it. Alternative:
     `std::process::exit(1)` as `stall` does. Rejected: an in-process test could not see it, and
     `AppExit` is Bevy's way.

3. **The env shot watches its capture.** `ShotWhenReady` keeps the `Shot` entity `capture`
   returned and checks it with the method's `result` each update. `Ok` removes the resource;
   `Err` fails as in 2, with `result`'s reason. One owner for the capture bounds: `result`.
   Fixed-step time stays stopped at `at` steps until the resource goes, as the spec already says
   ("until the shot is saved").

4. **Atomic write.** `capture` observes `save_atomically(path)`: it runs Bevy's
   `save_to_disk(temp)` (same encoding, same errors logged), then `std::fs::rename(temp, path)`.
   `temp` is `.<stem>.tmp-<pid>-<n>.<ext>` in `path`'s folder (`.` for a bare name): same
   filesystem so rename is atomic; a leading dot and a different name so a poller of `path`, or of
   `*.png`, never opens it; the extension kept so `save_to_disk` picks the same format. A failed
   rename logs an error and removes `temp`; `result` then reports "did not write". `asked` and the
   mtime check in `result` are unchanged: the renamed file's mtime is its write time.

## Does this cover the unreproduced "no shot in 120 s"?

Partly. The 120 s is `screenshot.rs`'s `a_dev_engine_takes_its_env_shot_once…` deadline; it
fails "no env.png within 120 s" only while the engine is alive, so Update kept running and the
watchdog did not fire. With the code today that leaves:

- ready not reached for 60 s (a cold build, or `bevy-ts dev`'s first compile under load), then a
  capture: written within a frame or two if Bevy renders, so this alone rarely makes 120 s;
- a capture that never writes: `READBACK_TIMEOUT` and `TIMEOUT` are checked only in the method's
  `result`, never for the env shot, so a readback that never comes or a failed `save_to_disk`
  waits for ever, silently. This is the likely case.
- the viewport path (Bevy marks a viewer-less viewport shot captured on its first frame) does not
  apply: the test runs `--headless`.

After this change the second case exits 1 with its reason within 5 s or 60 s of capture, so the
test fails "the engine exited" with a cause. The test must pipe the engine's stderr and print it
on failure to show it (task 3.3). A build that keeps progressing past 120 s still misses the
test's deadline: that is a slow machine, and now says so.

## Risks / Trade-offs

- [A build that progresses forever never fails] → the stall watchdog bounds a stopped Update; a
  running one that compiles a pipeline every 59 s is not a case seen.
- [Exit code 1 breaks a caller that treated any PNG as success] → that caller got a wrong picture
  before; CHANGELOG names it.
- [A game's `AppExit::Error` now changes the CLI's exit code] → Bevy's behaviour; CHANGELOG.
- [Temp files left by a crash mid-write] → dotfiles in the target folder, named for the pid;
  harmless, not swept.

## Migration Plan

Lands as one branch: code, tests, docs, CHANGELOG and spec. Rollback is a revert.

## Open Questions

None.
