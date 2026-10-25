## Context

`bevy-ts-cli`'s `base_app` (`src/cli.rs`) builds every engine: `headless::default_plugins`, then
bevy-ts, tsx and the built-in modules (`bevy-ts/input` in `src/input.rs` is the pattern: a plugin
that registers exported params and a module's JS and d.ts). Bevy's audio is compiled but unused,
and `AudioPlugin` opens the default output device when it is built (`AudioOutput::default`,
`bevy_audio/src/audio_output.rs:20-33`). Research: `docs/research/script-audio.md`.

Neighbours: reflect-coverage owns script inserts of reflect-tier components (stage 2) and
reflect-tier resources (stage 3); script-assets owns `Assets<T>.add`; writable-native-params owns
`#[export]`.

## Goals / Non-Goals

**Goals:**
- A game plays sound in a window, and in the studio's viewport when asked.
- No test, headless run or `bevy-ts types` opens an audio device.
- Every `bevy_audio` item has a TS form or a named owner, in one coverage table.
- Sink control from TS under Bevy's names, tested headless.

**Non-Goals:**
- A fake output device, or a copy of Bevy's playback systems.
- Spawning `AudioPlayer` from a script, `GlobalVolume`/`DefaultSpatialScale` writes, `Pitch`
  assets: their owners above.
- mp3 and flac by default (research §3).

## Coverage

`docs/research/script-audio.md` §4 is the table; task 4.1 copies it into `docs/audio.md`.

## Decisions

### Who adds `AudioPlugin`

`ScriptAudioPlugin` (`src/audio.rs`) is added in every mode. It registers `AudioPlayer` (generic,
so auto-registration skips it), the param and the module. Then by display:

- `Display::Window`: Bevy's `AudioPlugin`.
- `Display::Viewport`: `AudioPlugin` only with `--audio`; otherwise as headless.
- `Display::Headless` and `bevy-ts types`: the asset half, Bevy's own calls in Bevy's order:
  `init_asset::<AudioSource>()`, `init_asset_loader::<AudioLoader>()`,
  `insert_resource(GlobalVolume)`, `insert_resource(DefaultSpatialScale)`. No playback system,
  since none would run without a stream.

`bevy_audio` is a direct dependency (`default-features = false`, `vorbis`, `wav`), so
`DefaultPlugins` never has `AudioPlugin` and the one place it is added is `base_app`.
Alternative: Bevy's `audio` feature and `.disable::<AudioPlugin>()` everywhere a test builds
`DefaultPlugins`. Rejected: unification puts it in `bevy-ts-viewport`'s test apps, and the next
`DefaultPlugins` in a test opens the device. Ask-first: the lead's go.

Viewport is opt-in because a dozen tests start `--viewport` engines on the host. Alternative:
`--no-audio` in every test. Rejected: a test that forgets it opens the device and nothing fails.

### Decoders: `vorbis` and `wav`

`vorbis` is Bevy's default (`audio = ["bevy_audio", "vorbis"]`). `wav` is the format tools export
and the one a test fixture can be written by hand. mp3 and flac stay out: size in research §3,
and Ogg covers what they do. A project that needs one asks; a `bevy-ts.json` switch is a follow-up.

### One param over both sinks

`AudioSinks` in `bevy-ts/audio` is
`Query<(Option<&mut AudioSink>, Option<&mut SpatialAudioSink>, Has<AudioPlayer>)>`. Each
`AudioSinkPlayback` method takes the entity first and works on whichever sink it has, as Bevy's
trait does. `setEarsPosition`, `setListenerPosition` and `setEmitterPosition` need a spatial sink
and throw on a plain one. Names are Bevy's in camelCase.

An entity with `AudioPlayer` and no sink yet (loading, or no device) reads as not playing:
getters return `undefined`, setters do nothing. Bevy's own answer to "pause before it plays" is
`PlaybackSettings.paused`. An entity with no `AudioPlayer` throws, naming `AudioSinks`, the method
and the entity, as `Gamepads` does. Alternative: `With<AudioSink>` filters. Rejected: the sinks
have no `Reflect`, and a filter-only registration would be a new pattern.

Arguments cross as the export layer allows: `Volume` as its linear `number` (`Volume::Linear`;
the layer has no enums with data), `Duration` as seconds, `Transform` as translation and
rotation. `trySeek` throws Bevy's `SeekError` text.

### Tests are headless, with device-less sinks

`crates/bevy-ts-cli/tests/it/audio.rs`, a module of the crate's one binary:

- the mode rule: `base_app` headless and viewport-without-`--audio` have no `AudioPlugin` and an
  `ogg` loader; a test reads `is_plugin_added`, never opens a window;
- decoding: a fixture `.ogg` and a `.wav` the test writes load headless, and `Decodable::decoder()` yields samples
  at the file's rate and channel count;
- TSX: `<AudioPlayer value=..>` with `PlaybackSettings` props lands the handle's path and the
  hole values;
- the param: the test inserts `AudioSink::new(rodio::Player::new().0)` (and a spatial one) on an
  entity with `AudioPlayer`, a TS system calls each method, and Rust asserts the sink's state;
  `position` advances as the test pulls samples from the queue output.

The red-first test (task 1.1) asserts today's engine has no `ogg` loader and no `AudioPlayer`
type, then flips with the change.

## Risks / Trade-offs

- The studio's viewport is silent until abyme passes `--audio` → a follow-up item to abyme.
- Sink behaviour with a real device is never tested → Bevy owns it; the operator hears it.
- A sink-less entity reads as not playing, which hides "no device" → `docs/audio.md` says so.

## Migration Plan

Additive. Stage 1 lands alone. Stage 2 lands after reflect-coverage stages 2 and 3.

## Open Questions

- The two dependencies above: the lead's go.
