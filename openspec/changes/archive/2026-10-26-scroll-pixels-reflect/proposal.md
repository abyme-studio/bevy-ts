## Why

Bevy 0.20 reflects `MouseScrollPixelsPerLine` without `reflect(Resource)`, so `Res<T>` cannot reach
it and a game hard-codes its default of 100 to turn line scrolls into pixels. script-input-devices
listed it as an unreachable follow-up.

## What Changes

- `type_data` registers `ReflectResource` (and the `ReflectComponent` it depends on) for
  `MouseScrollPixelsPerLine`, beside the other type data Bevy leaves out. When Bevy reflects it
  itself, the redundancy check fails and the entry is deleted.
- `bevy-ts/bevy` gains `MouseScrollPixelsPerLine`, regenerated from the prebuilt engine: a script
  reads it with `Res` and sets it with `ResMut`, under Bevy's name.
- The other input resources script-input-devices exposes (`AccumulatedMouseMotion`,
  `AccumulatedMouseScroll`) already have `ReflectResource`: nothing to add.
- `docs/native.md`: the coverage rows and the scroll example read the resource instead of a constant.
- No protocol change.

## Capabilities

### Modified Capabilities

- `script-input`: `MouseScrollPixelsPerLine` is read and written through `Res`/`ResMut`, no longer
  unreachable.

## Impact

`crates/bevy-ts/src/type_data.rs`, `crates/bevy-ts-cli/tests/it/input_devices.rs`, the generated
`bevy-ts/bevy` files, `docs/native.md`, `openspec/specs/script-input/spec.md` at archive.
