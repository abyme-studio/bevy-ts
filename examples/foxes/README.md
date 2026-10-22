# Foxes

A grid of animated foxes, all in TSX. Each fox is a glTF scene placed with
`<WorldAssetRoot value="models/Fox.glb#Scene0" />`, and `<PlayAnimation clip=... />` beside it
names the clip to play. Rust finds the scene's animation player once the scene is ready and plays
the clip, the way Bevy's `many_foxes` example does. The button changes the clip on every fox, and
each cross-fades to it over `transition_secs`.

```sh
bevy-ts dev src/game.tsx
```

See `docs/animation.md` in bevy-ts for how `PlayAnimation` works.

## Credits

`assets/models/Fox.glb` comes from [Bevy](https://github.com/bevyengine/bevy) (`assets/models/animated/Fox.glb`):

- Model by [PixelMannen](https://opengameart.org/content/fox-and-shiba), licensed [CC0](https://creativecommons.org/publicdomain/zero/1.0/).
- Rigging and animation by [@tomkranis on Sketchfab](https://sketchfab.com/models/371dea88d7e04a76af5763f2a36866bc), licensed [CC-BY 4.0](https://creativecommons.org/licenses/by/4.0/).
