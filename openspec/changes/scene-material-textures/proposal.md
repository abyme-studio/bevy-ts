## Why

Agent town draws Synty models under a soft toon material (operator, 2026-10-23). `scene-material` puts a root's script material on every mesh of its glTF scene, but every mesh gets the same instance, so each mesh loses its own base-colour texture and colour. A toon material on a Synty scene needs each mesh's own albedo, from TS, with one `<material>` on the root.

Stage 2 of the toon look, after `script-material-passes` (stage 1, cast shadows and the prepass) lands.

## What Changes

- `<material kind gltf={{ <name>: <channel> }} />` on a scene root: each named texture or `color` param of the kind takes the swapped mesh's own value from its glTF `StandardMaterial`. Channels: `baseColor`, `baseColorTexture`, `emissive`, `emissiveTexture`, `normalMapTexture`, `metallicRoughnessTexture`, `occlusionTexture`. Every channel `StandardMaterial` reads from glTF, not one.
- A prop set on the element wins over `gltf` for the same name, so agent town's `game::MaskTint` keeps its explicit `mask` texture and tint colours while `base` comes from the mesh.
- Meshes that share a source `StandardMaterial` share one derived instance, so a Synty atlas scene makes one instance per glTF material, not per mesh.
- Changing the root's props re-derives every instance in the same frame; removing the material restores each mesh's own, as `scene-material` does.
- `gltf` joins `kind` and `gltfMaterial` as a reserved name `material()` refuses for a param or texture.
- Out of scope: per-mesh different kinds in one scene; `<material color>` (a `StandardMaterial`) on a scene root.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `script-materials`: a script material on a scene root can take each mesh's own glTF channels into its params and textures.

## Impact

- `crates/bevy-ts-material/src/scene.rs`: the swap derives one instance per (root material, source `StandardMaterial`); `GltfChannels` on the root.
- `crates/bevy-ts-tsx`: `resolve::script_material` takes `gltf` off the props; `insert_script_material` inserts or removes `GltfChannels`.
- `crates/bevy-ts-material/src/layout.rs`: `gltf` reserved. `packages/bevy-ts/jsx-runtime.d.ts`: `ScriptMaterialProps.gltf`, its keys typed by the kind's textures and `color` params.
- Tests: a two-material glTF scene keeps each texture under one toon kind; an explicit prop wins; a prop change and removal. Docs: docs/materials.md.
- No performance item: instances are per glTF material, as the scene already has.
