# script-audio: research

Research for coverage audit rows 158-159 (`docs/research/bevy-coverage.md`): Bevy audio from TS.
Read 2026-10-25 from Bevy 0.20.0's source (`~/dev/refs/bevy-0.20`, tag `latest` = commit `b3bdd75`
"Release 0.20.0", the version `Cargo.lock:477` pins), rodio 0.22.2 from the cargo registry, and
this tree at `0bf5e27a`. Paths below are under `crates/bevy_audio/src/` unless named.

## 1. Today: nothing can play

- `bevy_audio` is compiled, but by accident: `bevy_remote` → `bevy_dev_tools` → `bevy_audio`
  (`cargo tree --workspace -i bevy_audio -e features`; `bevy_dev_tools/Cargo.toml:29`, used by
  `bevy_dev_tools/src/inspection/label_resolution.rs:5` for labels only). rodio and cpal come with
  it (`Cargo.lock:6443`, `:2742`).
- Bevy's own `bevy_audio` *feature* is off: no bevy-ts crate names `audio`, `bevy_audio` or a
  decoder feature (every `Cargo.toml` in `crates/`). So `bevy::audio` does not exist
  (`bevy_internal/src/lib.rs:28`) and `DefaultPlugins` has no `AudioPlugin`
  (`bevy_internal/src/default_plugins.rs:80`).
- No decoder: no `lewton`, `hound`, `claxon` or `symphonia` in `Cargo.lock`. Without one
  `AudioPlugin` would not even register `AudioSource` or its loader (`lib.rs:97-101`).
- `PlaybackSettings` and `SpatialListener` are columns (`crates/bevy-ts-v8/js/bevy.js:31,33`,
  `packages/bevy-ts/bevy.d.ts:66-79`) because reflection auto-registers them. `AudioPlayer` is
  generic (`audio.rs:251`), so auto-registration skips it: TS cannot name it.

A test that proves nothing plays, at the lowest layer (a unit test in `bevy-ts-cli`, over the
app `base_app` builds headless):

- the `AssetServer` has no loader for `ogg` (`get_asset_loader_with_extension("ogg")` fails);
- the type registry has no `bevy_audio::audio::AudioPlayer`;
- `Assets<AudioSource>` is not a resource.

The change's first task writes it, and it turns green to red to green: after the change the
loader and the type are there, and the same test asserts so.

## 2. Headless: how sinks are made

`AudioPlugin::build` (`lib.rs:81-105`) inserts `GlobalVolume` and `DefaultSpatialScale`, adds
`play_queued_audio_system` and `cleanup_finished_audio` per source type, and
`init_resource::<AudioOutput>()`.

- `AudioOutput::default()` opens the default output device at plugin build:
  `DeviceSinkBuilder::open_default_sink()` (`audio_output.rs:20-33`). With no device it logs "No
  audio device found." and keeps `None`. On this Mac it opens CoreAudio's default output.
- `AudioOutput` is `pub(crate)` (`audio_output.rs:16`): nothing outside Bevy can give it a fake
  or silent stream.
- Every playback system runs only `if audio_output_available` (`lib.rs:88`, `audio_output.rs:326`).
  `play_queued_audio_system` returns at once with no stream (`audio_output.rs:103-106`). So with
  no device **no `AudioSink` or `SpatialAudioSink` is ever inserted**: an `AudioPlayer` stays
  queued forever, and `PlaybackMode::Despawn`/`Remove` never fire.
- With a device, the system waits for the asset, builds `rodio::Player::connect_new(mixer)` (or
  `SpatialPlayer`), appends the decoder shaped by `mode`, `start_position` and `duration`, applies
  `muted`, `speed`, `volume * GlobalVolume` and `paused`, and inserts the sink
  (`audio_output.rs:109-281`). `PlaybackSettings` is read once; later changes do nothing
  (`audio.rs:30-32`). Spatial sinks follow `GlobalTransform` and `SpatialListener` each frame
  after `TransformSystems::Propagate` (`audio_output.rs:331-374`, `lib.rs:89`).

A sink needs no device to exist. `AudioSink::new(Player)` and `SpatialAudioSink::new(SpatialPlayer)`
are public (`sinks.rs:158`, `:262`), and rodio's `Player::new()` returns a player and its
`SourcesQueueOutput` with no device (`rodio-0.22.2/src/player.rs:81`). Bevy's own sink test does
exactly this (`sinks.rs:413-418`). A test that pulls samples from the queue output advances the
sink as a device would.

What a test can read from a sink (`AudioSinkPlayback`, `sinks.rs:10-126`): `volume`, `speed`,
`is_paused`, `is_muted`, `position`, `empty`. `position` moves only as samples are pulled, so a
device-less test pulls them; `empty` turns true once the queue drains or `stop` runs.

Device rule: nothing may open an audio device on the host Mac, so:

- Headless (`--headless`, `bevy-ts types`, every test) never adds `AudioPlugin`. It adds Bevy's
  asset half instead (`AudioSource`, `AudioLoader`, the two resources, the type registrations),
  so assets load, types resolve, and no sink is made.
- `--viewport` engines run in tests on the host (`play_stop.rs`, `dev_edit.rs`, ten more), so
  viewport mode opens no device unless told to: `--audio`, which abyme's bevy-engine plugin
  (`~/dev/abyme/web/plugins/src/bevy-engine/api.server.ts`) passes when it launches a game for
  the operator.
- Window mode (`bevy-ts dev` in a window, `bevy-ts play`, a shipped game) adds `AudioPlugin`.
  Tests never open a window on the host, so none run there.
- Sink behaviour is tested headless with device-less sinks: a test inserts
  `AudioSink::new(Player::new().0)` and drives it. That needs rodio by name: Bevy re-exports
  `Source` and `Sample` (`lib.rs:55`) but not `Player` or `SpatialPlayer`. rodio is already in
  `Cargo.lock` at the version Bevy pins, so a dev-dependency adds no crate, but it is a direct
  dependency beyond Bevy's features: **ask-first**.

A tart VM does not help. `tart run` passes the guest's audio to the host's speakers unless
`--no-audio` (`tart run --help`), so a guest with sound opens the host device; with `--no-audio`
the guest has no output device and Bevy makes no sinks, as headless. A guest-side virtual device
(BlackHole) would be new infrastructure, not proposed. That sound comes out of the speakers is
the operator's check at the Mac, not a test.

## 3. Decoders

Bevy's feature → rodio's (`bevy_audio/Cargo.toml` `[features]`, `rodio-0.22.2/Cargo.toml`):

| Bevy feature | rodio feature | Crate | Extensions |
|---|---|---|---|
| `vorbis` (in Bevy's `audio`) | `lewton` | lewton | `ogg`, `oga`, `spx` |
| `wav` | `hound` | hound | `wav` |
| `flac` | `claxon` | claxon | `flac` |
| `mp3` | `mp3` → `symphonia-mp3` | symphonia (core, bundle-mp3) | `mp3` |
| `symphonia-vorbis`, `-wav`, `-flac` | `vorbis`, `wav`, `flac` | symphonia | as above |
| `mp4`, `aac` | `symphonia-isomp4`, `-aac` | symphonia | none in the loader: `AudioLoader::extensions` lists neither (`audio_source.rs:59-74`) |

Size each adds to a release binary: `bench/audio-size` (an app with only `AudioPlugin`), one
release build per feature, `bench/audio-size/sizes.sh`, run as the `audio-size` process in a
check slot. Sizes are deterministic, so one build per arm is the measurement.

Measured 2026-10-25 (`target/audio-size.log`), macOS arm64, release, bytes after `strip`; "adds"
is over the `none` arm, which already has `AudioPlugin`, rodio and cpal:

| Arm | Stripped | Adds | In or out |
|---|---|---|---|
| none | 8,903,376 | — | — |
| `vorbis` | 9,520,960 | 603 KiB | in: Bevy's default, Ogg is the game format |
| `wav` | 9,386,992 | 472 KiB | in: what tools export; +50 KiB beside `vorbis` |
| `vorbis` + `wav` | 9,571,136 | 652 KiB | the choice |
| `flac` | 9,420,224 | 505 KiB | out: lossless is for masters, not shipped games |
| `mp3` | 9,806,768 | 882 KiB | out: the largest; Ogg does what it does |
| `symphonia-vorbis`, `-wav`, `-flac` | 8,903,5xx | ~0 | out: Bevy registers `AudioSource` and its loader only under `mp3`, `flac`, `wav` or `vorbis` (`lib.rs:97`), so a symphonia feature alone links no decoder and loads nothing |

Most of each first decoder is shared: `AudioSource`, its loader and rodio's `Decoder` (~430 KiB, the
smallest single arm less what `wav` adds beside `vorbis`). A release `bevy-ts` engine is about 240 MB unstripped
(other worktrees' `target/release/bevy-ts`, 2026-10-25), so `vorbis` + `wav` is about 0.3% of it.

## 4. Coverage

Every public item of `bevy_audio` 0.20 (`lib.rs:42-56` and each module), its TS form, status and
test. "Column" is the layout tier, "reflect" the reflect tier (docs/native.md, "Rust components
in TS"); a param is an `#[export]` param (writable-native-params).

| Bevy item | TS form | Status | Test / owner |
|---|---|---|---|
| `AudioPlugin { global_volume, default_spatial_scale }` | added by the engine (window; viewport with `--audio`); headless gets its asset half | missing | this change: `audio::` mode tests |
| `AudioPlayer<AudioSource>(Handle)` | reflect: `<AudioPlayer value="sounds/hit.ogg" />`; `With<AudioPlayer>` | missing: generic, never registered | this change (registration, TSX); `commands.spawn` of it: reflect-coverage stage 2.3 |
| `AudioSource { bytes }`, `AudioLoader` | an asset path | missing (no decoder) | this change: an `.ogg` and a `.wav` load and decode headless |
| `Decodable`, `AddAudioSource`, `Source`, `Sample`, `SampleRate`, `ChannelCount`, `CpalSample` | none: Rust extension points | — | — |
| `Pitch { frequency, duration }` | `AudioPlayer<Pitch>` needs `Assets<Pitch>::add` | missing | script-assets (`Assets<T>.add`); registration here |
| `PlaybackSettings { mode, speed, paused, muted, spatial }` | column, today | partial | this change: test |
| `PlaybackSettings { volume, spatial_scale, start_position, duration }` | holes; TSX props through reflection (`volume={{ Linear: 0.5 }}`, `start_position={1.5}`) | partial | this change: TSX test; script `.of` with holes: reflect-coverage stage 2.4 |
| `PlaybackSettings::ONCE/LOOP/DESPAWN/REMOVE`, `paused()`, `muted()`, `with_*` | `mode` and fields: `PlaybackSettings.of({ mode: "Loop" })` | — | no consts: Bevy's are struct literals, the fields say it |
| `PlaybackMode` | fieldless enum, `"Once" \| "Loop" \| "Despawn" \| "Remove"` | done (column field) | this change: test |
| `SpatialListener { left_ear_offset, right_ear_offset }`, `new(gap)` | column | partial: no test | this change: test; `new(gap)` is two `Vec3`s |
| `SpatialScale(Vec3)`, `new`, `new_2d` | reflect value inside `PlaybackSettings.spatial_scale` and `DefaultSpatialScale` | — | with its owners |
| `DefaultSpatialScale(SpatialScale)` | resource, reflect tier | missing | reflect-coverage stage 3 (reflected resources); docs and test here, stage 2 |
| `GlobalVolume { volume }`, `new` | resource, reflect tier (`Volume` has data) | missing | reflect-coverage stage 3; docs and test here, stage 2 |
| `Volume::{Linear, Decibels}` | reflect shape `{ Linear: n } \| { Decibels: n }`; params cross linear `number` | — | this change |
| `Volume::to_linear`, `to_decibels`, `increase_by_percentage`, `decrease_by_percentage`, `scale_to_factor`, `fade_towards`, `*`, `/`, `SILENT` | none: TS math on numbers | — | declined: recipes |
| `AudioSink` (no `Reflect`) | param `AudioSinks`, entity first | missing | this change |
| `SpatialAudioSink` (no `Reflect`) | the same param: each method works on either sink | missing | this change |
| `AudioSinkPlayback::volume`, `set_volume` | `volume(e)`, `setVolume(e, linear)` | missing | this change |
| `speed`, `set_speed` | `speed(e)`, `setSpeed(e, speed)` | missing | this change |
| `play`, `pause`, `toggle_playback`, `is_paused` | `play(e)`, `pause(e)`, `togglePlayback(e)`, `isPaused(e)` | missing | this change |
| `position`, `try_seek` | `position(e)` seconds; `trySeek(e, seconds)` throws Bevy's `SeekError` text | missing | this change |
| `stop`, `empty` | `stop(e)`, `empty(e)` | missing | this change |
| `is_muted`, `mute`, `unmute`, `toggle_mute` | `isMuted(e)`, `mute(e)`, `unmute(e)`, `toggleMute(e)` | missing | this change |
| `SpatialAudioSink::set_ears_position`, `set_listener_position`, `set_emitter_position` | `setEarsPosition(e, left, right)`, `setListenerPosition(e, translation, rotation, gap)`, `setEmitterPosition(e, position)`; a plain sink throws | missing | this change |
| `PlaybackDespawnMarker`, `PlaybackRemoveMarker` | none: Bevy internals, no `Reflect` | — | — |
| `SeekError` | the thrown message | — | this change |

`AudioSink` and `SpatialAudioSink` have no `Reflect`, so TS cannot name them in `With`. The param
finds the sink itself: an entity with an `AudioPlayer` and no sink yet (loading, or no output
device) reads as not playing (getters `undefined`, setters do nothing); an entity with no
`AudioPlayer` throws, as `Gamepads` does for a non-pad.

## 5. Neighbours

- reflect-coverage stage 2 owns inserting reflect-tier components from scripts (`AudioPlayer`)
  and layout-tier `.of` with holes (`PlaybackSettings.volume`); stage 3 owns reflect-tier
  resources (`GlobalVolume`, `DefaultSpatialScale`).
- script-assets owns `Assets<T>.add` (`Pitch`) and load state.
- writable-native-params owns the `#[export]` rules; the param here uses `Query<AnyOf<(&mut
  AudioSink, &mut SpatialAudioSink)>>`, a shape it already supports (`Query<&mut T>`).
- abyme's bevy-engine plugin passes `--audio` to the viewport engine it launches for the operator.
