## 1. Stage 1: the engine plays, the module controls

- [x] 1.1 Red first: `crates/bevy-ts-cli/tests/it/audio.rs` (and `mod audio;` in `tests/it/main.rs`) asserts a headless `base_app` has an `ogg` and a `wav` loader and an `AudioPlayer` type; it fails on main
- [x] 1.2 `crates/bevy-ts-cli/Cargo.toml`: `bevy_audio = "=0.20.0"` (`default-features = false`, `vorbis`, `wav`); dev-dependency `rodio = "=0.22.2"` (after the lead's go)
- [x] 1.3 `crates/bevy-ts-cli/src/audio.rs`: `ScriptAudioPlugin` registers `AudioPlayer` and the module `bevy-ts/audio`; `audio_output(app, display, audio)` adds `AudioPlugin` or the asset half
- [x] 1.4 `crates/bevy-ts-cli/src/cli.rs`: `--audio` on `bevy-ts dev`; `base_app` adds `ScriptAudioPlugin` and calls the mode rule; `bevy-ts types` gets the asset half
- [x] 1.5 Mode tests in `audio.rs`: headless and viewport-without-`--audio` have no `AudioPlugin`; a window plays (checked on `plays`: building `AudioPlugin` would open the device)
- [x] 1.6 Fixture `crates/bevy-ts-cli/tests/it/fixtures/tone.ogg` (a short tone, under 8 KB); the test writes its `.wav` itself; decode test
- [x] 1.7 TSX test: `<AudioPlayer value="tone.ogg" />` with `PlaybackSettings` `mode="Loop"`, `volume={{ Decibels: -6 }}`, `start_position={0.5}`; Rust reads the handle's path and the fields
- [x] 1.8 `AudioSinks` param in `audio.rs`: every `AudioSinkPlayback` method and the three spatial setters, entity first; sink-less reads as not playing; no `AudioPlayer` throws
- [x] 1.9 Param tests on device-less sinks: each method, `position` after pulled samples, `empty` after `stop`, `trySeek` error text, spatial setters on a plain sink throw
- [x] 1.10 `scripts/gen-bevy-types.sh`: regenerate `crates/bevy-ts-v8/js/bevy.js` and `packages/bevy-ts/bevy.d.ts` with `AudioPlayer`
- [x] 1.11 `docs/audio.md`: the mode rule, TSX, the param with an example, the coverage table; `docs/research/bevy-coverage.md` rows 158-159 point at it
- [x] 1.12 Give abyme a follow-up item: its bevy-engine plugin passes `--audio` to the viewport engine
- [x] 1.13 An `AudioSource` Bevy cannot decode (`music.mp3`) is removed on load and named in the script log, instead of Bevy panicking when it plays; tests `an_mp3_is_refused_by_name`, and `a_reloaded_source_is_checked_again` (a hot reload keeps the id and replaces the bytes, so the check runs again on new bytes)
- [x] 1.14 `AudioPlayer` has no `Default`: the engine registers a `ReflectFromWorld` for it, and TSX starts a component from `ReflectDefault`, else `ReflectFromWorld` (`reflect_value::default_of`); test `tsx_writes_audio_player_and_playback_settings`

## 2. Stage 2: scripts and resources (after reflect-coverage stages 2 and 3)

- [ ] 2.1 Test `commands.spawn(AudioPlayer.of("tone.ogg"), PlaybackSettings.of({ mode: "Despawn", volume: { Linear: 0.5 } }))` from a TS system
- [ ] 2.2 Test `ResMut<GlobalVolume>` and `Res<DefaultSpatialScale>` from TS; document that `GlobalVolume` applies to sinks made after it changes
- [ ] 2.3 Mark their table rows done in `docs/audio.md`
