## Why

A ready-frame shot can be wrong or missing with no error. `BEVY_TS_SCREENSHOT` waits at most 60 s
from start for its ready frame, then shoots anyway: on a cold build that is still compiling
pipelines, that saves a picture of fallbacks and the clear colour, with only a warning. Once it
has captured, nothing bounds it: the no-frame (5 s) and readback (60 s) limits are checked only
by `bevy-ts/screenshot`, so an env shot Bevy never writes waits for ever while Update runs. And
both write the PNG in place with Bevy's `save_to_disk`, so a reader polling the path can read a
half-written file.

## What Changes

- The 60 s ready limit, for the env shot and the method, counts time with no progress, not time
  since start. Progress is a pipeline Bevy's `PipelineCache` finished, or fewer asset loads in
  flight than the fewest since the last progress. A cold build that keeps compiling is waited
  out; a hang (a missing shader, a load that never ends, a script that never starts) is not.
- **BREAKING**: `BEVY_TS_SCREENSHOT` no longer shoots anyway. When it cannot be ready, or Bevy
  renders, reads back or writes nothing in time, it logs an error naming the path and the reason,
  saves no file, and the run exits with code 1. `bevy-ts play` and `bevy-ts dev` exit with the
  code of Bevy's `AppExit`, so a game's own `AppExit::Error` now sets it too.
- The env shot keeps watching its capture with the method's bounds: no frame within 5 s, no image
  read back within 60 s, captured but not written.
- Both shots write to a temp file in the target's folder (`.<stem>.tmp-<pid>-<n>.<ext>`), then
  rename it onto the target. A reader of the path sees the old file or the whole new one, never
  part of one.
- No new environment variable. A test sets the no-progress window through a resource.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `headless-run`: the env shot waits out a cold build, fails loudly instead of shooting anyway,
  bounds its capture, and writes atomically.
- `remote-protocol`: `bevy-ts/screenshot`'s ready limit restarts on progress, and it writes
  atomically.

## Impact

- `crates/bevy-ts-remote/src/screenshot.rs`: progress clock, `ReadyTimeout`, atomic save, env
  shot's capture bounds and `AppExit`.
- `crates/bevy-ts-cli/src/play.rs`, `crates/bevy-ts-cli/src/cli.rs`: exit with `app.run()`'s code.
- Tests: `crates/bevy-ts-remote/src/screenshot.rs` (unit), `crates/bevy-ts-cli/tests/it/screenshot_ready.rs`,
  `crates/bevy-ts-cli/tests/it/screenshot.rs`.
- `docs/build.md` (Headless paragraph), `CHANGELOG.md`.
- Behaviour change needing the operator's go: an env shot that cannot be ready now exits 1 with
  no file instead of saving a wrong picture, and a game's `AppExit::Error` now sets
  `bevy-ts play`'s and `bevy-ts dev`'s exit code. Agent town's shot runs, and any script that
  treats a saved PNG as success, see the failure instead. No ask-first item is touched: no new
  dependency, service, env var or pattern; no performance cost beyond one rename per shot.
