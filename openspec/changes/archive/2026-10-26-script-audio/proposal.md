## Why

Nothing can play a sound in a bevy-ts game. `bevy_audio` is compiled only because `bevy_dev_tools`
uses it; Bevy's `audio` feature is off, so no `AudioPlugin`, no decoder, and `AudioPlayer` (a
generic type reflection never registers) cannot be named from TS. Coverage audit rows 158-159 ask
for Bevy's whole audio category from TS. Research, with file:line evidence, is in
[docs/research/script-audio.md](../../../docs/research/script-audio.md).

## What Changes

- The engine plays audio: Bevy's `AudioPlugin`, with the `vorbis` and `wav` decoders (sizes in the
  research), added in window mode, and in viewport mode only with `--audio`. Headless, `bevy-ts
  types` and every test add only its asset half (`AudioSource`, `AudioLoader`, `GlobalVolume`,
  `DefaultSpatialScale`), so assets load and types resolve and no audio device opens.
- `AudioPlayer` is registered, so TSX writes `<AudioPlayer value="sounds/hit.ogg" />` and a query
  filters `With<AudioPlayer>`. `PlaybackSettings` stays a column; its holes (`volume`,
  `spatial_scale`, `start_position`, `duration`) are TSX props through reflection.
- A new built-in module `bevy-ts/audio` with one exported param, `AudioSinks`, over both
  `AudioSink` and `SpatialAudioSink`: every `AudioSinkPlayback` method and the three spatial
  setters, under Bevy's names, entity first.
- Docs: `docs/audio.md`, with the coverage table of every `bevy_audio` 0.20 item, its TS form and
  its owner; the audit rows marked.
- Tests in `crates/bevy-ts-cli/tests/it/audio.rs`: the mode rule, decoding, TSX, and the param on
  device-less sinks.
- Not here: spawning `AudioPlayer` from a script and `PlaybackSettings.of` with holes
  (reflect-coverage stage 2), `GlobalVolume` and `DefaultSpatialScale` from TS (reflect-coverage
  stage 3; stage 2 here documents and tests them), `AudioPlayer<Pitch>` (script-assets'
  `Assets<T>.add`), `Volume`'s arithmetic (TS math on numbers).

## Capabilities

### New Capabilities

- `script-audio`: audio playback in the engine, the device rule, `AudioPlayer` and
  `PlaybackSettings` from TSX, and the `AudioSinks` param.

### Modified Capabilities

None.

## Impact

- Code: `crates/bevy-ts-cli/Cargo.toml`, new `crates/bevy-ts-cli/src/audio.rs`,
  `crates/bevy-ts-cli/src/cli.rs` (the mode rule, `--audio`), regenerated
  `crates/bevy-ts-v8/js/bevy.js` and `packages/bevy-ts/bevy.d.ts`, new
  `crates/bevy-ts-cli/tests/it/audio.rs` and two small fixtures, `docs/audio.md`,
  `docs/research/bevy-coverage.md`.
- abyme: its bevy-engine plugin passes `--audio` to the viewport engine it launches for the
  operator (`~/dev/abyme/web/plugins/src/bevy-engine/api.server.ts`). A follow-up item for abyme;
  until then the studio's viewport is silent.
- Ask-first, waiting on the lead's go:
  - Dependency: `bevy_audio = "=0.20.0"` (features `vorbis`, `wav`) as a direct dependency of
    `bevy-ts-cli`, not Bevy's `audio` feature. Bevy's own crate, already in `Cargo.lock`; no new
    crate except the decoders. Why: the feature puts `AudioPlugin` into every `DefaultPlugins`
    in the workspace build (feature unification), and `bevy-ts-viewport`'s tests use
    `DefaultPlugins`, so they would open the Mac's audio device. Alternative: the feature, and
    `.disable::<AudioPlugin>()` in `headless::default_plugins` and five test helpers, with
    `bevy-ts-viewport` dev-enabling the feature so they compile alone; a new `DefaultPlugins`
    in a test would then open the device again.
  - Dev-dependency: `rodio = "=0.22.2"` in `bevy-ts-cli`, for `rodio::Player::new()`, a sink with
    no device (Bevy's own sink test does this). Already in `Cargo.lock`. Without it no test can
    make a sink: Bevy re-exports neither `Player` nor `SpatialPlayer`.
  - Size: the decoders add to every engine and shipped game (research §3).
- Performance: none at rest. Bevy's playback systems run only with an output device; the param is
  one fast op per call.
