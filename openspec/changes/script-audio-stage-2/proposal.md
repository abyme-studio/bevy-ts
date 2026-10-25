## Why

script-audio stage 1 (archived as `2026-10-26-script-audio`) plays sound and controls it from
`bevy-ts/audio`. Two rows of `docs/audio.md`'s coverage table wait on reflect-coverage: spawning
`AudioPlayer` from a script with `PlaybackSettings.of` and its holes (reflect-coverage stage 2), and
`GlobalVolume` and `DefaultSpatialScale` from TS (reflect-coverage stage 3). This change is that
stage 2, split out so stage 1 could archive.

## What Changes

- Tests: a TS system spawns `AudioPlayer.of(...)` with `PlaybackSettings.of({...})`, and reads and
  writes `ResMut<GlobalVolume>` and `Res<DefaultSpatialScale>`.
- Docs: `docs/audio.md` says `GlobalVolume` applies to sinks made after it changes, and marks the
  rows done.
- No engine code is expected: reflect-coverage stages 2 and 3 bring the paths. If a test shows a
  gap, the gap is reflect-coverage's.

## Capabilities

### Modified Capabilities

- `script-audio`: its coverage rows for script spawns and the two resources become done. Spec
  deltas, if any, are written when the work starts, against what reflect-coverage landed.

## Impact

`crates/bevy-ts-cli/tests/it/audio.rs`, `docs/audio.md`. Starts after reflect-coverage stages 2
and 3 are on main.
