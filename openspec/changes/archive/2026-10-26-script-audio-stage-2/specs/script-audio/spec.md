## ADDED Requirements

### Requirement: AudioPlayer and PlaybackSettings from a script
A TS system SHALL spawn and insert a sound with Bevy's names: `AudioPlayer.of(path)` takes the asset path, and `PlaybackSettings.of({...})` takes its column fields and its reflected ones (`volume`, `spatial_scale`, `start_position`, `duration`) in the shape TSX takes.

#### Scenario: A one-shot hit at half volume
- **WHEN** a `Startup` system runs `commands.spawn(AudioPlayer.of("tone.ogg"), PlaybackSettings.of({ mode: "Despawn", volume: { Linear: 0.5 }, spatial_scale: [2, 2, 2], start_position: 0.25 }))`
- **THEN** one entity has an `AudioPlayer` whose handle has path `tone.ogg` and `PlaybackSettings` with `mode` `Despawn`, `volume` `Linear(0.5)`, `spatial_scale` 2 on each axis, `start_position` 0.25 s and no `duration`

### Requirement: GlobalVolume and DefaultSpatialScale from TS
`bevy-ts/bevy` SHALL export `GlobalVolume` and `DefaultSpatialScale`, taken as `Res` and `ResMut` and read and written through reflection: `GlobalVolume.volume` as `{ Linear: n }` or `{ Decibels: n }`, `DefaultSpatialScale.value` as `[x, y, z]`. `docs/audio.md` SHALL say that `GlobalVolume` applies to sinks made after it changes, as Bevy multiplies it in when it makes a sink.

#### Scenario: Quieter from a script
- **WHEN** a system with `ResMut<GlobalVolume>` sets `volume` to `{ Linear: 0.25 }`, and `DefaultSpatialScale` is 0.5 on each axis
- **THEN** Rust's `GlobalVolume` is `Linear(0.25)` after that frame, and the system reads `DefaultSpatialScale.value` as `[0.5, 0.5, 0.5]`
