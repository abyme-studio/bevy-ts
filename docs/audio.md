# Audio

A game plays sound with Bevy's own audio: `AudioPlayer` and `PlaybackSettings` in TSX, and the
param `AudioSinks` from `bevy-ts/audio` to control a sound once it plays. The engine decodes Ogg
Vorbis (`ogg`, `oga`, `spx`) and WAV (`wav`).

## Where sound plays

Bevy's `AudioPlugin` opens the default output device when it is built. So the engine adds it only
where someone listens:

| Engine | Plays sound |
|---|---|
| A window: `bevy-ts dev`, `bevy-ts play`, a shipped game | yes |
| `bevy-ts dev --viewport` (the studio's viewport) | only with `--audio` |
| `bevy-ts dev --headless`, `bevy-ts play --headless`, `bevy-ts types`, tests | no |

Where it does not play, the engine still has Bevy's `AudioSource` asset and its loader, and the
`GlobalVolume` and `DefaultSpatialScale` resources: audio assets load and audio types resolve. No
device opens, and Bevy makes no sinks, so `AudioSinks` reads every sound as not playing yet.
`--audio` without `--viewport` is refused: a window always plays, and headless never does.

## Play a sound from TSX

```tsx
import { app } from "bevy-ts";
import { AudioPlayer, PlaybackSettings } from "bevy-ts/bevy";

app.scene("level", () => (
  <entity name="music">
    <AudioPlayer value="sounds/music.ogg" />
    <PlaybackSettings mode="Loop" volume={{ Decibels: -6 }} start_position={0.5} />
  </entity>
));
```

`value` is the asset path, under the project's `assets/`. `PlaybackSettings` has Bevy's fields:
`mode` (`"Once"`, `"Loop"`, `"Despawn"`, `"Remove"`), `speed`, `paused`, `muted`, `spatial`,
`volume` (`{ Linear: n }` or `{ Decibels: n }`), `spatial_scale`, and `start_position` and
`duration` in seconds (`null` for none). Bevy reads them once, when the sound starts; change a
playing sound through `AudioSinks`. A query filters sounds with `With<AudioPlayer>`.

## Control a playing sound

```ts
import type { Entity, Query, With } from "bevy-ts";
import { AudioPlayer } from "bevy-ts/bevy";
import { KeyCode, type Keys } from "bevy-ts/input";
import type { AudioSinks } from "bevy-ts/audio";

export function music(keys: Keys, sinks: AudioSinks, sounds: Query<[Entity], With<AudioPlayer>>) {
  sounds.each((e) => {
    if (keys.justPressed(KeyCode.KeyM)) sinks.toggleMute(e);
    if (keys.justPressed(KeyCode.Space)) sinks.togglePlayback(e);
    if (keys.pressed(KeyCode.ArrowUp)) sinks.setVolume(e, Math.min(1, (sinks.volume(e) ?? 1) + 0.01));
  });
}
```

`AudioSinks` stands for Bevy's `AudioSink` and `SpatialAudioSink`: each method takes the entity
first and acts on whichever sink it has, under Bevy's `AudioSinkPlayback` names:

| Method | Does |
|---|---|
| `volume(e)`, `setVolume(e, linear)` | the volume, linear (`1` is the source's own, `0` or more); while muted, the volume `unmute` restores |
| `speed(e)`, `setSpeed(e, speed)` | the speed, above `0`, `1` normal; it moves the pitch too |
| `play(e)`, `pause(e)`, `togglePlayback(e)`, `isPaused(e)` | pause and resume |
| `position(e)` | seconds played |
| `trySeek(e, seconds)` | seeks; throws Bevy's `SeekError` text when the source cannot |
| `stop(e)`, `empty(e)` | stops for good; whether nothing is left to play |
| `isMuted(e)`, `mute(e)`, `unmute(e)`, `toggleMute(e)` | mute, keeping the volume |
| `setEarsPosition(e, left, right)` | a spatial sound's ears, in world space |
| `setListenerPosition(e, translation, rotation, gap)` | the listener: an ear each side, `gap` apart |
| `setEmitterPosition(e, position)` | a spatial sound's emitter |

A volume crosses as its linear `number`; Bevy's `Volume` arithmetic (`to_decibels`,
`fade_towards`, ...) is TS math on that number.

- A sound whose asset is still loading has no sink yet: getters return `undefined`, setters do
  nothing. With no output device (headless, or a machine with none) every sound stays so. To start
  a sound paused, give it `PlaybackSettings paused`.
- An entity with no `AudioPlayer` throws, naming `AudioSinks`, the method and the entity. So does a
  volume below 0, a speed of 0 or less, a negative seek, or NaN.
- The three spatial setters throw on a plain sink: play it with `PlaybackSettings spatial`. Bevy
  also moves a spatial sound's emitter to its `GlobalTransform` and the ears to the
  `SpatialListener`'s each frame.

## Formats

Ogg Vorbis and WAV are compiled in: Ogg for music and effects, WAV for what tools export. mp3 and
flac are not: together they would add 1.4 MB to every engine (docs/research/script-audio.md §3).

An `AudioPlayer`'s asset loads by its type, whatever its extension, so a `music.mp3` loads, and
Bevy would panic when it plays it. The engine removes such a source when it loads, or when a hot
reload replaces it, and logs an
error naming it: `sounds/music.mp3 is not Ogg Vorbis or WAV, the audio this engine decodes`. A
`.ogg` or `.wav` whose bytes are not that format is refused the same way.

To play mp3 or flac, turn on Bevy's decoder in a project engine (docs/native.md): a native crate
of the project that depends on `bevy_audio` with the feature, which Cargo adds to the engine's
`bevy_audio`:

```toml
# native/sound/Cargo.toml
[dependencies]
bevy_audio = { version = "=0.20.0", default-features = false, features = ["mp3", "flac"] }
```

Then `AudioLoader` lists `mp3` and `flac`, and the engine plays them. Or convert the file:
`ffmpeg -i music.mp3 -c:a libvorbis music.ogg`.

## Coverage

Every public item of `bevy_audio` 0.20, its TS form, and the test or owner. Tests are in
`crates/bevy-ts-cli/tests/it/audio.rs`.

| Bevy item | TS form | Status | Test / owner |
|---|---|---|---|
| `AudioPlugin { global_volume, default_spatial_scale }` | added by the engine where it plays; elsewhere its asset half | done | `only_a_window_or_a_viewport_given_audio_plays`, `a_headless_engine_loads_audio_and_names_audio_player` |
| `AudioPlayer<AudioSource>(Handle)` | `<AudioPlayer value="sounds/hit.ogg" />`; `With<AudioPlayer>` | done in TSX | `tsx_writes_audio_player_and_playback_settings`; `commands.spawn` of it: reflect-coverage stage 2, then script-audio-stage-2 |
| `AudioSource { bytes }`, `AudioLoader` | an asset path | done | `ogg_and_wav_decode_headless`, `an_mp3_is_refused_by_name`, `a_reloaded_source_is_checked_again` |
| `Decodable`, `AddAudioSource`, `Source`, `Sample`, `SampleRate`, `ChannelCount`, `CpalSample` | none: Rust extension points | — | — |
| `Pitch { frequency, duration }`, `AudioPlayer<Pitch>` | needs `Assets<Pitch>.add` from TS | missing | script-assets |
| `PlaybackSettings { mode, speed, paused, muted, spatial }` | column fields | done | `tsx_writes_audio_player_and_playback_settings` |
| `PlaybackSettings { volume, spatial_scale, start_position, duration }` | TSX props through reflection | done in TSX | `tsx_writes_audio_player_and_playback_settings`; `PlaybackSettings.of` with them: reflect-coverage stage 2 |
| `PlaybackSettings::ONCE`, `LOOP`, `DESPAWN`, `REMOVE`, `paused()`, `muted()`, `with_*` | the fields: `mode="Loop"`, `paused` | — | Bevy's constants are struct literals |
| `PlaybackMode` | `"Once" \| "Loop" \| "Despawn" \| "Remove"` | done | `tsx_writes_audio_player_and_playback_settings` |
| `SpatialListener { left_ear_offset, right_ear_offset }`, `new(gap)` | column; `new(gap)` is the two offsets | done (column) | registered by reflection; behaviour is Bevy's, with a device |
| `SpatialScale`, `new`, `new_2d` | a value in `PlaybackSettings.spatial_scale` and `DefaultSpatialScale` | — | with those |
| `DefaultSpatialScale` | `Res<DefaultSpatialScale>` | missing | reflect-coverage stage 3, then script-audio-stage-2 |
| `GlobalVolume { volume }`, `new` | `ResMut<GlobalVolume>`; applies to sinks made after it changes | missing | reflect-coverage stage 3, then script-audio-stage-2 |
| `Volume::{Linear, Decibels}` | `{ Linear: n } \| { Decibels: n }`; params take linear `number`s | done | `tsx_writes_audio_player_and_playback_settings`, `audio_sinks_control_sinks_with_no_device` |
| `Volume::to_linear`, `to_decibels`, `increase_by_percentage`, `decrease_by_percentage`, `scale_to_factor`, `fade_towards`, `*`, `/`, `SILENT` | TS math on numbers | declined | — |
| `AudioSink`, `SpatialAudioSink` (no `Reflect`) | `AudioSinks`, entity first | done | `audio_sinks_control_sinks_with_no_device` |
| `AudioSinkPlayback::volume`, `set_volume`, `speed`, `set_speed` | `volume`, `setVolume`, `speed`, `setSpeed` | done | `audio_sinks_control_sinks_with_no_device` |
| `play`, `pause`, `toggle_playback`, `is_paused` | `play`, `pause`, `togglePlayback`, `isPaused` | done | `audio_sinks_control_sinks_with_no_device` |
| `position`, `try_seek` | `position` (seconds), `trySeek` | done | `audio_sinks_control_sinks_with_no_device` |
| `stop`, `empty` | `stop`, `empty` | done | `audio_sinks_control_sinks_with_no_device` |
| `is_muted`, `mute`, `unmute`, `toggle_mute` | `isMuted`, `mute`, `unmute`, `toggleMute` | done | `audio_sinks_control_sinks_with_no_device` |
| `SpatialAudioSink::set_ears_position`, `set_listener_position`, `set_emitter_position` | `setEarsPosition`, `setListenerPosition`, `setEmitterPosition`; a plain sink throws | done | `audio_sinks_control_sinks_with_no_device` |
| `SeekError` | the thrown message | done | `audio_sinks_control_sinks_with_no_device` |
| `PlaybackDespawnMarker`, `PlaybackRemoveMarker` | none: Bevy's internals | — | — |

That sound comes out of the speakers is the one thing no test hears: every test runs without a
device, and Bevy owns playback.
