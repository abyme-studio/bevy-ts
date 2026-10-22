## Why

`<material>` sets five of `StandardMaterial`'s fields: `color`, `metallic`, `roughness`, `emissive` and `unlit` (material-alpha adds `alpha` and `alphaMode`). A game that wants a textured crate, a normal-mapped wall, a two-sided leaf, glass or car paint must write Rust today. Bevy already draws all of it; TSX only has to say it. This is row 13 of the bevy-coverage audit.

## What Changes

- **`<material>` takes every other `StandardMaterial` field**, under Bevy's field name in camelCase: textures, their UV channels, `reflectance`, `specularTint`, transmission and thickness, `ior`, attenuation, `flipNormalMapY`, clearcoat, anisotropy, `doubleSided`, `cullMode`, `fogEnabled`, `depthBias`, parallax, `lightmapExposure`, `opaqueRenderMethod`, `deferredLightingPassId` and `uvTransform`. The full list, with each prop and its test, is the coverage table in `design.md`.
- **Textures are asset paths**: `baseColorTexture="textures/crate.png"`, loaded by the `AssetServer` as `<image src>` and script-material textures are. Colour textures (base colour, emissive, specular tint) load as sRGB; data textures (normal, metallic-roughness, occlusion, depth and the rest) load as linear, as Bevy's glTF loader does. A path that fails to load is reported at the element.
- **Enums use the style `alphaMode` uses on the same element**: `cullMode="back" | "front" | null`, `baseColorChannel="uv0" | "uv1"`, `opaqueRenderMethod="forward" | "deferred" | "auto"`, `parallaxMappingMethod="occlusion" | { relief: { maxSteps: 5 } }`.
- **`uvTransform`** is `{ scale?, rotation?, translation? }`, Bevy's `Affine2::from_scale_angle_translation`.
- **`<mesh>` meshes carry tangents**, so a normal map or anisotropy shades a `<mesh>` shape. Bevy ignores both on a mesh without them.
- **Four Bevy features turn on** so every field exists: `pbr_transmission_textures`, `pbr_specular_textures`, `pbr_multi_layer_material_textures`, `pbr_anisotropy_texture`. Without them Bevy compiles those texture fields out.
- Existing props keep their names: `color` is `base_color`, `roughness` is `perceptual_roughness`. No second name is added.
- Typed in `bevy-ts/jsx-runtime`: `MaterialProps` gains every prop, with its enum and object shapes.
- **Out of scope:** `alpha`, `alphaMode` (material-alpha); a texture's sampler (repeat, filtering), so a `uvTransform` scale above 1 clamps rather than tiles until image load settings exist; a texture given as an asset handle rather than a path (script-assets); binding `<material>` props to functions.

## Capabilities

### New Capabilities

None. material-alpha creates `standard-materials`; this change adds to it.

### Modified Capabilities

- `standard-materials`: `<material>` takes every `StandardMaterial` field, textures as asset paths, and a `<mesh>` shows a normal map.

## Impact

- Code: `crates/bevy-ts-tsx/src/intrinsics.rs` (`MaterialProps` gains the fields, their checks and `material()`; texture loading; tangents in `mesh_handle`); `crates/bevy-ts-tsx/src/reconcile.rs` for a texture's load failure at the element; `crates/bevy-ts-tsx/Cargo.toml` (the four Bevy features).
- Types: `packages/bevy-ts/jsx-runtime.d.ts` (`MaterialProps`); `packages/bevy-ts/test/tsx/elements.tsx`.
- Docs: `docs/materials.md`, a "Standard materials" section with every prop.
- Tests: `crates/bevy-ts-tsx/tests/standard_material.rs` (every prop to its field, textures, errors, tangents); headless render cases in `crates/bevy-ts-cli/tests/materials.rs`.
- Protocol: none. `<material>` props do not cross it.
- Neighbours: builds on material-alpha (rebase after it lands; reuses its `AlphaDecl` style for enums). Colours (`specularTint`, `attenuationColor`) go through `intrinsics.rs`'s `color()`, which reflect-enums-color repoints at `bevy_ts::color`. Tangents are added after mesh-primitives' builders, in `mesh_handle`, so the two do not overlap.
- No new dependency or service.

### Needs the operator's go

1. **The four Bevy PBR texture features.** Every `StandardMaterial` bind group, glTF ones included, gains up to eight texture and sampler pairs, bound to fallbacks when unset. Cost: a larger bind group per material and a longer shader build; some platforms limit sampled textures per stage (WebGL2's 16), which bevy-ts does not target. Without the go, the eight feature-gated textures and their channels (rows marked "feature" in the coverage table) drop from this change; everything else stands.
2. **Tangents on every `<mesh>` mesh.** One more vertex attribute (16 bytes a vertex) on each cached shape. Meshes are shared by props, so this is memory per distinct shape, not per entity. Without it, normal maps and anisotropy do nothing on `<mesh>` shapes (they still work on glTF meshes with tangents).
