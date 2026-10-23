## Why

`<material>` sets five of `StandardMaterial`'s fields: `color`, `metallic`, `roughness`, `emissive` and `unlit` (material-alpha adds `alpha` and `alphaMode`). A game that wants a textured crate, a normal-mapped wall, a two-sided leaf, glass or car paint must write Rust today. Bevy already draws all of it; TSX only has to say it. This is row 13 of the bevy-coverage audit.

## What Changes

- **`<material>` takes every other `StandardMaterial` field Bevy compiles by default**, under Bevy's field name in camelCase: textures, their UV channels, `reflectance`, `specularTint`, transmission and thickness, `ior`, attenuation, `flipNormalMapY`, clearcoat, anisotropy, `doubleSided`, `cullMode`, `fogEnabled`, `depthBias`, parallax, `lightmapExposure`, `opaqueRenderMethod`, `deferredLightingPassId` and `uvTransform`. The full list, with each prop and its test, is the coverage table in `design.md`.
- **Textures are asset paths**: `baseColorTexture="textures/crate.png"`, loaded by the `AssetServer` as `<image src>` and script-material textures are. Colour textures (base colour, emissive) load as sRGB; data textures (normal, metallic-roughness, occlusion, depth) load as linear, as Bevy's glTF loader does. A path that fails to load is reported at the element.
- **Enums use the style `alphaMode` uses on the same element**: `cullMode="back" | "front" | null`, `baseColorChannel="uv0" | "uv1"`, `opaqueRenderMethod="forward" | "deferred" | "auto"`, `parallaxMappingMethod="occlusion" | { relief: { maxSteps: 5 } }`.
- **`uvTransform`** is `{ scale?, rotation?, translation? }`, Bevy's `Affine2::from_scale_angle_translation`.
- **A `<mesh>` gets tangents when its material needs them.** Bevy ignores a normal map or anisotropy on a mesh without tangents. When the entity's `<material>` sets `normalMapTexture` or anisotropy, its shape is swapped for a copy with tangents, built once per shape and shared. A plain mesh stays as it is.
- **Nine textures are blocked on Bevy.** Transmission, thickness, specular, specular tint, clearcoat (three) and anisotropy textures, and their UV channels, exist only behind four Bevy features (`pbr_transmission_textures`, `pbr_specular_textures`, `pbr_multi_layer_material_textures`, `pbr_anisotropy_texture`). With them on, Bevy's opaque pipeline binds 18 textures and 18 samplers, past wgpu's default 16, so every `StandardMaterial` fails on a device at the default limits. The features stay off; the coverage table marks the 18 fields blocked, with an upstream issue. Their scalar fields ship.
- Existing props keep their names: `color` is `base_color`, `roughness` is `perceptual_roughness`. No second name is added.
- Typed in `bevy-ts/jsx-runtime`: `MaterialProps` gains every prop, with its enum and object shapes.
- **Out of scope:** `alpha`, `alphaMode` (material-alpha); a texture's sampler (repeat, filtering), so a `uvTransform` scale above 1 clamps rather than tiles until image load settings exist; a texture given as an asset handle rather than a path (script-assets); binding `<material>` props to functions.

## Capabilities

### New Capabilities

None. material-alpha creates `standard-materials`; this change adds to it.

### Modified Capabilities

- `standard-materials`: `<material>` takes every `StandardMaterial` field Bevy compiles by default, textures as asset paths, and a `<mesh>` shows a normal map.

## Impact

- Code: `crates/bevy-ts-tsx/src/intrinsics.rs` (`MaterialProps` gains the fields, their checks and `material()`; texture loading; tangents in `mesh_handle`, and the tangent swap between an entity's `<mesh>` and `<material>`); `crates/bevy-ts-tsx/src/reconcile.rs` for a texture's load failure at the element.
- Types: `packages/bevy-ts/jsx-runtime.d.ts` (`MaterialProps`); `packages/bevy-ts/test/tsx/elements.tsx`.
- Docs: `docs/materials.md`, a "Standard materials" section with every prop.
- Tests: `crates/bevy-ts-tsx/tests/standard_material.rs` (every prop to its field, textures, errors, tangents); headless render cases in `crates/bevy-ts-cli/tests/materials.rs`; `crates/bevy-ts-cli/tests/standard_material_limits.rs` (the pipeline fits wgpu's default limits). Bench: `bench/standard-material-features/`.
- Protocol: none. `<material>` props do not cross it.
- Neighbours: builds on material-alpha (rebase after it lands; reuses its `AlphaDecl` style for enums). Colours (`specularTint`, `attenuationColor`) go through `intrinsics.rs`'s `color()`, which reflect-enums-color repoints at `bevy_ts::color`. Tangents are added after mesh-primitives' builders, in `mesh_handle`, so the two do not overlap.
- No new dependency or service.

### Ask-first items

lead@bevy-ts answered both on 2026-10-23:

1. **The four Bevy PBR texture features: dropped.** Approved on a measuring condition; the measurement found no frame or GPU cost, but the opaque pipeline binds 18 textures and 18 samplers against wgpu's default 16 per stage (`bench/standard-material-features/results/README.md`). The features stay off and their nine textures are blocked on Bevy.
2. **Tangents, lazily.** Only a `<mesh>` whose entity's material needs them gets them, from a per-shape cache. 16 bytes a vertex per distinct shape that uses a normal map or anisotropy; nothing for any other mesh.
