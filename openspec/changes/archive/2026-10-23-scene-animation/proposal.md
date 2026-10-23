## Why

A game cannot play a glTF's animation from TS today. Agent town's villagers stay cubes until it can. Bevy's own pattern (`animated_mesh.rs`, `many_foxes.rs`) is Rust wiring: spawn `WorldAssetRoot`, wait for `WorldInstanceReady`, find the `AnimationPlayer` under it, give it a graph and `AnimationTransitions`, and play. A TS game should get that by placing one component, with Rust doing the wiring.

## What Changes

- A new reflected Rust component, `PlayAnimation { clip, repeat, speed, transition_secs }`, set on a scene root beside `WorldAssetRoot`. `clip` is a `Handle<AnimationClip>`, written as an asset path (`"models/Fox.glb#Animation2"`).
- A Rust plugin plays it: once the scene is ready, it finds every `AnimationPlayer` under the root, gives each one shared `AnimationGraph` and `AnimationTransitions`, and plays the clip. A changed `clip` cross-fades to the new one over `transition_secs`. A changed `speed` or `repeat` applies to the playing clip. Removing the component stops the players.
- It is placed like any reflect-tier component: as a TSX component element (`<PlayAnimation clip="..." />`), from a level file, or through the editor. No special element, no TS method calls.
- TSX: a Rust component that is a one-field tuple struct takes that field as the prop `value`, as `Name` does. This lets TSX place `<WorldAssetRoot value="models/Fox.glb#Scene0" />`, which it cannot today.
- The asset preview's walk for `AnimationPlayer`s under a root moves to the new crate, so one place owns it.
- A new shipped example, `foxes`: a grid of animated foxes in TSX, with a button that switches their clip.
- Docs (`docs/animation.md`) and a measured cost: TS form against a Rust baseline and static models, at 50, 100 and 200 characters.

Out of scope: calling Rust component methods from TS (a later change), TS observers on Rust entity events, a descendants walk from TS, and inserting reflect-tier components from `Commands`. This change needs none of them.

## Capabilities

### New Capabilities
- `scene-animation`: the `PlayAnimation` component and the Rust plugin that plays a glTF clip on a scene's animation players.

### Modified Capabilities
- `jsx-runtime`: a one-field tuple-struct Rust component takes its field as the prop `value`.
- `example-projects`: the package ships a third example, `foxes`.

## Impact

- New crate `crates/bevy-ts-animation`, built like `bevy-ts-material`, added to the engine in `bevy-ts-cli` (`cli.rs`, `types.rs`). It uses Bevy's `bevy_animation` and `bevy_gltf`, which the `3d` feature already builds. No new dependency.
- `crates/bevy-ts-tsx/src/resolve.rs`: the `value` prop for one-field tuple structs.
- `crates/bevy-ts-viewport/src/preview.rs`: calls the shared player walk; preview behaviour is unchanged.
- `bevy-ts/bevy` regenerates (`scripts/gen-bevy-types.sh`) to list `PlayAnimation`.
- Remote protocol: no change. The new component is reached through the existing reflected-component methods.
- A third-party asset in the npm package: Bevy's `Fox.glb` (CC0 model by PixelMannen; CC-BY 4.0 rigging and animation by @tomkranis), credited in the example's README.
- Performance: one observer run per scene spawn, one system over `Changed<PlayAnimation>`; nothing per frame from TS. Measured by the bench in tasks.
