# script-audio Specification

## Purpose
Sound in a bevy-ts game: Bevy's `AudioPlugin` where a game is heard (a window, or a viewport given `--audio`) and its asset half everywhere else, Ogg Vorbis and WAV decoding with undecodable sources refused by name, `AudioPlayer` and `PlaybackSettings` in TSX, and `bevy-ts/audio`'s `AudioSinks` to control a playing sound.

## Requirements

### Requirement: Audio plays in a window, never in a test
The engine SHALL add Bevy's `AudioPlugin` in window mode, and in viewport mode only when `bevy-ts dev` is given `--audio`. Headless runs, `bevy-ts types`, and viewport mode without `--audio` SHALL NOT add it and SHALL open no audio device; they SHALL add Bevy's `AudioSource` asset, its `AudioLoader`, and the `GlobalVolume` and `DefaultSpatialScale` resources, so audio assets load and audio types resolve. Every mode SHALL register `AudioPlayer`.

#### Scenario: A headless run
- **WHEN** an engine starts with `--headless`
- **THEN** it has no `AudioPlugin`, its asset server has loaders for `ogg` and `wav`, and its type registry has `AudioPlayer`

#### Scenario: A viewport engine started by a test
- **WHEN** a test starts `bevy-ts dev --viewport` without `--audio`
- **THEN** the engine has no `AudioPlugin` and opens no audio device

#### Scenario: A shipped game
- **WHEN** a game runs in its own window
- **THEN** the engine has Bevy's `AudioPlugin`, and an `AudioPlayer` whose asset has loaded gets an `AudioSink` when an output device exists

### Requirement: Ogg and WAV decode
The engine SHALL decode Ogg Vorbis (`ogg`, `oga`, `spx`) and WAV (`wav`) through Bevy's `vorbis` and `wav` features. mp3 and flac SHALL NOT be compiled in. Since an `AudioPlayer`'s asset loads by type whatever its extension, an `AudioSource` Bevy's decoder cannot take (its extension is not one `AudioLoader` lists, or an Ogg or WAV path whose bytes are not that container) SHALL be removed when it loads, or when a hot reload replaces it under the same id, and named in the script log, before Bevy plays it that frame, so Bevy never panics decoding it.

#### Scenario: Decode headless
- **WHEN** a headless engine loads `tone.ogg` and `tone.wav`
- **THEN** each is an `AudioSource` whose decoder yields samples at the file's sample rate and channel count

#### Scenario: An mp3
- **WHEN** a headless engine loads `music.mp3` as an `AudioSource`
- **THEN** its bytes load, the engine removes the source, and the script log has one error naming `music.mp3`, "Ogg Vorbis or WAV" and `docs/audio.md`

#### Scenario: An mp3 saved over an Ogg file
- **WHEN** `tone.ogg` has loaded and its source is replaced under the same id by mp3 bytes, as a hot reload does
- **THEN** the engine removes the source, and the script log has one error naming `tone.ogg`

### Requirement: AudioPlayer and PlaybackSettings from TSX
`bevy-ts/bevy` SHALL declare `AudioPlayer` as a reflect-tier component whose one field is `value`, an asset path, so `<AudioPlayer value="sounds/hit.ogg" />` plays that asset and `With<AudioPlayer>` filters a query. `PlaybackSettings` SHALL stay a column (`mode`, `speed`, `paused`, `muted`, `spatial`), with `volume` (`{ Linear: n }` or `{ Decibels: n }`), `spatial_scale`, `start_position` and `duration` (seconds, or `null`) as TSX props written through reflection.

#### Scenario: Looping music at -6 dB
- **WHEN** TSX renders `<entity><AudioPlayer value="music.ogg" /><PlaybackSettings mode="Loop" volume={{ Decibels: -6 }} start_position={0.5} /></entity>`
- **THEN** the entity's `AudioPlayer` handle has path `music.ogg` and its `PlaybackSettings` has `mode` `Loop`, `volume` `Decibels(-6)` and `start_position` 0.5 s

### Requirement: AudioSinks controls a playing sound
`bevy-ts/audio` SHALL export the param `AudioSinks`, over `AudioSink` and `SpatialAudioSink`. Each method SHALL take the entity first and act on whichever sink it has, under Bevy's `AudioSinkPlayback` names: `volume`, `setVolume`, `speed`, `setSpeed`, `play`, `pause`, `togglePlayback`, `isPaused`, `position`, `trySeek`, `stop`, `empty`, `isMuted`, `mute`, `unmute`, `toggleMute`. A volume SHALL cross as its linear `number` and a position as seconds. `trySeek` SHALL throw Bevy's `SeekError` text. A volume below 0, a speed of 0 or less, a negative number of seconds, or NaN SHALL throw and change nothing. `setEarsPosition(e, left, right)`, `setListenerPosition(e, translation, rotation, gap)` and `setEmitterPosition(e, position)` SHALL act on a `SpatialAudioSink` and throw on a plain sink. An entity with an `AudioPlayer` and no sink yet SHALL read as not playing: getters return `undefined`, setters do nothing. An entity with no `AudioPlayer` SHALL throw, naming `AudioSinks`, the method and the entity.

#### Scenario: Pause and resume
- **WHEN** an entity has an `AudioPlayer` and an `AudioSink`, and a system calls `sinks.pause(e)`
- **THEN** the sink is paused that frame, `sinks.isPaused(e)` is true, and `sinks.togglePlayback(e)` resumes it

#### Scenario: Mute keeps the volume
- **WHEN** a system calls `sinks.setVolume(e, 0.5)`, then `sinks.mute(e)`
- **THEN** `sinks.isMuted(e)` is true and `sinks.volume(e)` is 0.5, as Bevy's managed volume

#### Scenario: Position moves with playback
- **WHEN** a sink plays a 44.1 kHz mono source and 22,050 samples are pulled from it
- **THEN** `sinks.position(e)` is 0.5

#### Scenario: Not loaded yet
- **WHEN** an entity has an `AudioPlayer` whose asset is still loading
- **THEN** `sinks.isPaused(e)` is `undefined` and `sinks.pause(e)` does nothing

#### Scenario: Not a sound
- **WHEN** a system calls `sinks.pause(player)` on an entity with no `AudioPlayer`
- **THEN** the call throws naming `AudioSinks`, `pause` and the entity

#### Scenario: Ears on a plain sink
- **WHEN** a system calls `sinks.setEmitterPosition(e, [1, 0, 0])` on an entity with a plain `AudioSink`
- **THEN** the call throws naming `AudioSinks`, `setEmitterPosition` and that the sink is not spatial

### Requirement: Every Bevy audio item is accounted for
`docs/audio.md` SHALL list every public item of `bevy_audio` 0.20 with its TS form, status and owner, and a test SHALL exist for every row marked done.

#### Scenario: A row without a test
- **WHEN** a reviewer reads a row marked done
- **THEN** the row names the test in `crates/bevy-ts-cli/tests/it/audio.rs` that covers it

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
