## 1. Scripts and resources (after reflect-coverage stages 2 and 3)

- [ ] 1.1 Test `commands.spawn(AudioPlayer.of("tone.ogg"), PlaybackSettings.of({ mode: "Despawn", volume: { Linear: 0.5 } }))` from a TS system
- [ ] 1.2 Test `ResMut<GlobalVolume>` and `Res<DefaultSpatialScale>` from TS; document that `GlobalVolume` applies to sinks made after it changes
- [ ] 1.3 Mark their table rows done in `docs/audio.md`
