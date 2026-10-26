## 1. Scripts and resources (after reflect-coverage stages 2 and 3)

- [x] 1.1 Test `commands.spawn(AudioPlayer.of("tone.ogg"), PlaybackSettings.of({ mode: "Despawn", volume: { Linear: 0.5 } }))` from a TS system
- [x] 1.2 Test `ResMut<GlobalVolume>` and `Res<DefaultSpatialScale>` from TS; document that `GlobalVolume` applies to sinks made after it changes
- [x] 1.3 Mark their table rows done in `docs/audio.md`

## 2. Gaps the tests showed

- [x] 2.1 `bevy-ts/bevy` and `native.d.ts` ship reflect-tier resources as reflected handles (`types.rs`), except one a param stands for (`GizmoConfigStore`); regenerate; `docs/native.md`
- [x] 2.2 TSX and levels build a `FromWorld` default that needs resources from the live world, as scripts do (`bevy-ts-tsx/src/reflected.rs`); test in `bevy-ts-tsx/tests/it/from_world.rs`; `docs/reflection.md`
- [x] 2.3 Spec deltas: `script-audio`, `native-components`
