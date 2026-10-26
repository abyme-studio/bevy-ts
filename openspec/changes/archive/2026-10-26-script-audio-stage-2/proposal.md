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
- Engine: reflect-tier resources had no `bevy-ts/bevy` handle, so `Res<GlobalVolume>` could not be
  imported; `bevy-ts types` now ships them (not one a param stands for). TSX built a `FromWorld`
  default only on an empty world; one that needs resources is now built from the live world, as
  scripts' commands do.

## Capabilities

### Modified Capabilities

- `script-audio`: script spawns of `AudioPlayer` and `PlaybackSettings`, and `GlobalVolume` and
  `DefaultSpatialScale` from TS.
- `native-components`: reflect-tier resources are reflected handles; TSX builds a `FromWorld`
  default from the live world.

## Impact

`crates/bevy-ts-cli/tests/it/audio.rs`, `docs/audio.md`, `crates/bevy-ts-cli/src/types.rs` and the
regenerated `bevy-ts/bevy`, `crates/bevy-ts-tsx/src/reflected.rs`, `docs/native.md`,
`docs/reflection.md`.
