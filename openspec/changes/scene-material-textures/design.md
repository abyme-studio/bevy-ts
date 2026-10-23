## Context

`scene-material` (`crates/bevy-ts-material/src/scene.rs`) swaps each mesh of a root's scene to the root's one `ScriptMaterial3d`, keeping the mesh's `StandardMaterial` handle in `SceneMaterialTargets` to restore it. A `ScriptMaterial` instance holds its kind, its uniform bytes and its textures, so per-mesh values need per-mesh (per source material) instances.

## Goals / Non-Goals

**Goals:** each mesh keeps its glTF albedo (and the other channels) under one script kind, from TS, with no Rust per game. **Non-Goals:** several kinds per scene; deriving anything a glTF material does not hold.

## Decisions

1. A map on the element (`gltf={{ name: channel }}`), not a naming convention (a texture called `base_color_texture`). The author keeps their WGSL names, and one kind can be used both on scenes and on plain meshes.
2. Derived instances are keyed by (root instance, source `StandardMaterial` asset id) in a cache on the root, so an atlas scene with one glTF material makes one instance. They are built from the root's instance by overwriting the mapped params and textures, read from `Assets<StandardMaterial>` when the swap runs; a source material not loaded yet retries next frame, as a texture does today.
3. The channel list is every texture and colour `StandardMaterial` takes from glTF, so a later toon or PBR-ish kind needs no new channel.
4. Coordination with `scene-material`'s tint use (`game::MaskTint`): unchanged without `gltf`; with it, props win, so the explicit `mask` and tints stay.

## Risks / Trade-offs

- A scene with many glTF materials makes many instances: one per material, which is what `StandardMaterial` already costs.
- glTF `KHR_texture_transform` UV transforms on a channel are not carried; noted in docs as a limit.
