## Context

`<material>` is a serde struct, `MaterialProps` in `crates/bevy-ts-tsx/src/intrinsics.rs`, with `deny_unknown_fields`. `check()` validates at load; `material()` builds a `StandardMaterial` from `default()`; `cache_key()` is the props' `Debug`, so equal props share one handle in `IntrinsicAssets`. It sets `base_color`, `metallic`, `perceptual_roughness`, `emissive` and `unlit`. material-alpha (not landed) adds `alpha` and `alphaMode`, with lowercase camelCase mode names parsed by `bevy-ts-material`'s `AlphaDecl`.

Textures already load from paths in two places: `<image src>` (`ui.rs`) and script-material textures (`kinds.rs`), both `AssetServer::load(path)`.

Bevy 0.20 compiles eight texture fields, and their UV channels, only behind four features: `pbr_transmission_textures`, `pbr_specular_textures`, `pbr_multi_layer_material_textures`, `pbr_anisotropy_texture`. The workspace turns none on. Bevy's primitive meshes carry no tangents, and Bevy's shader ignores a normal map or anisotropy without them.

## Goals / Non-Goals

**Goals:** every `StandardMaterial` field reachable from `<material>`, typed, checked at load, and each covered by a test.

**Non-Goals:** `alpha` and `alphaMode` (material-alpha); image sampler settings; texture handles from TS (script-assets); bindable `<material>` props; `<mesh color>` gaining these props (it stays a colour shorthand).

## Decisions

1. **Hand-written fields on `MaterialProps`, not a reflection writer.** `StandardMaterial` is `Reflect`, and reflect-enums-color brings a reflect writer. But textures need loading with per-field colour spaces, two names (`color`, `roughness`) differ from Bevy's, and enum names follow `alphaMode`'s style on this element. One serde field per prop keeps the check, the type and the test side by side, as today. The coverage table below is the list a reviewer checks against Bevy's struct.
2. **Names are Bevy's field names in camelCase.** `depth_map` is `depthMap`, `normal_map_texture` is `normalMapTexture`. `color` and `roughness` keep their shipped names; adding `baseColor` and `perceptualRoughness` would give one field two owners.
3. **Enums in lowercase camelCase, as `alphaMode`.** reflect-enums-color writes components' enums in Bevy's reflected shape (`"Back"`); `<material>` is not a component and already says `alphaMode="blend"`. One element, one style. `cullMode={null}` is `None`, as reflect-enums-color writes an `Option`. Serde enums with `rename_all = "camelCase"` parse them, so an unknown name fails at load with the names listed.
4. **Colour space by field, as Bevy's glTF loader.** `baseColorTexture`, `emissiveTexture` and `specularTintTexture` load with default settings (sRGB). Every other texture loads with `load_with_settings` and `ImageLoaderSettings { is_srgb: false, .. }`. Risk: one file used both ways keeps the settings of its first load (Bevy keys a handle by path); docs say to keep colour and data textures in separate files, as any glTF does.
5. **A texture that fails to load is reported at the element.** `insert_material` records each texture handle with the element's `SourceLocation`; a system on `AssetLoadFailedEvent<Image>` logs through `reconcile::log_error`, naming the path. The material keeps drawing what Bevy draws for an unloaded texture.
6. **`uvTransform` as scale, rotation, translation.** `Affine2`'s reflected shape is a 2x2 matrix and a translation, which nobody writes by hand. `{ scale, rotation, translation }` is Bevy's own `from_scale_angle_translation`. Unset parts are identity.
7. **Tangents only when the material needs them, cached per shape.** Mesh and material are sibling elements, each cached by its own props, and either may be inserted or changed first. So each records what it is on the entity: `insert_mesh` keeps the `<mesh>` props in a private `IntrinsicMesh` component; `insert_material` sets a private `NeedsTangents` marker when the props set `normalMapTexture`, `clearcoatNormalTexture`, `anisotropyTexture` or a non-zero `anisotropyStrength`, and removes it otherwise (and `remove_material` and a script material remove it). Both then call `fit_mesh(world, e)`, which inserts the plain handle or the tangent handle for `IntrinsicMesh`'s props. `IntrinsicAssets` keeps tangent meshes in their own map under the same key, built once from the plain mesh with `with_generated_tangents()`, after whatever builder mesh-primitives puts in `mesh()`; a shape it fails on maps to the plain handle. `<mesh color>`'s shorthand material never needs tangents. Cost: 16 bytes a vertex per distinct shape with such a material, nothing for the rest.
8. **The four Bevy features, on in `bevy-ts-tsx`.** Feature unification turns them on for the prebuilt engine and shipped games. Approved on a condition, checked before landing (task 5): a measurement of the arena plus 100 distinct materials, features on and off, 3 runs per arm, frame time and GPU time, kept under `bench/standard-material-features/` with its results; and a count of the `StandardMaterial` pipeline's sampled textures and samplers per shader stage, across all its bind groups, against `wgpu::Limits::default()` (16 and 16), plus pipeline creation on each of the 5 release platforms. A measurable cost goes to lead@bevy-ts before landing.
9. **Checks.** Finite numbers; colours through `color()` (reflect-enums-color repoints it at `bevy_ts::color`, so `specularTint` gains every colour form with no work here); enums and `uvTransform` by serde; `deferredLightingPassId` is a `u8`; texture paths non-empty; an `AssetServer` present when any texture is set. Ranges Bevy does not check (a `metallic` of 2) are not checked, as today.

## Coverage table

Every field of Bevy 0.20's `StandardMaterial`. "feature" marks a field behind one of the four features. Tests are in `crates/bevy-ts-tsx/tests/standard_material.rs` unless named: `fields` is the table-driven `every_prop_sets_its_field` (one case per prop, named by the prop); `textures` is `every_texture_loads_its_path_in_its_colour_space` (one case per texture); render tests are in `crates/bevy-ts-cli/tests/materials.rs`.

| `StandardMaterial` field | TS prop | Type | Test |
|---|---|---|---|
| `base_color` | `color` (shipped) | colour | fields |
| `base_color_channel` | `baseColorChannel` | `"uv0" \| "uv1"` | fields |
| `base_color_texture` | `baseColorTexture` | path, sRGB | textures; render `a_textured_cube_draws_the_image` |
| `emissive` | `emissive` (shipped) | colour | fields |
| `emissive_exposure_weight` | `emissiveExposureWeight` | number | fields |
| `emissive_channel` | `emissiveChannel` | uv channel | fields |
| `emissive_texture` | `emissiveTexture` | path, sRGB | textures |
| `perceptual_roughness` | `roughness` (shipped) | number | fields |
| `metallic` | `metallic` (shipped) | number | fields |
| `metallic_roughness_channel` | `metallicRoughnessChannel` | uv channel | fields |
| `metallic_roughness_texture` | `metallicRoughnessTexture` | path, linear | textures |
| `reflectance` | `reflectance` | number | fields |
| `specular_tint` | `specularTint` | colour | fields |
| `diffuse_transmission` | `diffuseTransmission` | number | fields |
| `diffuse_transmission_channel` | `diffuseTransmissionChannel` | uv channel, feature | fields |
| `diffuse_transmission_texture` | `diffuseTransmissionTexture` | path, linear, feature | textures |
| `specular_transmission` | `specularTransmission` | number | fields |
| `specular_transmission_channel` | `specularTransmissionChannel` | uv channel, feature | fields |
| `specular_transmission_texture` | `specularTransmissionTexture` | path, linear, feature | textures |
| `thickness` | `thickness` | number | fields |
| `thickness_channel` | `thicknessChannel` | uv channel, feature | fields |
| `thickness_texture` | `thicknessTexture` | path, linear, feature | textures |
| `ior` | `ior` | number | fields |
| `attenuation_distance` | `attenuationDistance` | number | fields |
| `attenuation_color` | `attenuationColor` | colour | fields |
| `normal_map_channel` | `normalMapChannel` | uv channel | fields |
| `normal_map_texture` | `normalMapTexture` | path, linear | textures; render `a_normal_map_shades_a_cube` |
| `flip_normal_map_y` | `flipNormalMapY` | boolean | fields |
| `occlusion_channel` | `occlusionChannel` | uv channel | fields |
| `occlusion_texture` | `occlusionTexture` | path, linear | textures |
| `specular_channel` | `specularChannel` | uv channel, feature | fields |
| `specular_texture` | `specularTexture` | path, linear, feature | textures |
| `specular_tint_channel` | `specularTintChannel` | uv channel, feature | fields |
| `specular_tint_texture` | `specularTintTexture` | path, sRGB, feature | textures |
| `clearcoat` | `clearcoat` | number | fields |
| `clearcoat_channel` | `clearcoatChannel` | uv channel, feature | fields |
| `clearcoat_texture` | `clearcoatTexture` | path, linear, feature | textures |
| `clearcoat_perceptual_roughness` | `clearcoatPerceptualRoughness` | number | fields |
| `clearcoat_roughness_channel` | `clearcoatRoughnessChannel` | uv channel, feature | fields |
| `clearcoat_roughness_texture` | `clearcoatRoughnessTexture` | path, linear, feature | textures |
| `clearcoat_normal_channel` | `clearcoatNormalChannel` | uv channel, feature | fields |
| `clearcoat_normal_texture` | `clearcoatNormalTexture` | path, linear, feature | textures |
| `anisotropy_strength` | `anisotropyStrength` | number | fields |
| `anisotropy_rotation` | `anisotropyRotation` | number (radians) | fields |
| `anisotropy_channel` | `anisotropyChannel` | uv channel, feature | fields |
| `anisotropy_texture` | `anisotropyTexture` | path, linear, feature | textures |
| `double_sided` | `doubleSided` | boolean | fields; render `a_two_sided_plane_draws_its_back` |
| `cull_mode` | `cullMode` | `"back" \| "front" \| null` | fields; render `a_two_sided_plane_draws_its_back` |
| `unlit` | `unlit` (shipped) | boolean | fields |
| `fog_enabled` | `fogEnabled` | boolean | fields |
| `alpha_mode` | `alphaMode` (material-alpha) | — | material-alpha |
| `depth_bias` | `depthBias` | number | fields |
| `depth_map` | `depthMap` | path, linear | textures |
| `parallax_depth_scale` | `parallaxDepthScale` | number | fields |
| `parallax_mapping_method` | `parallaxMappingMethod` | `"occlusion" \| { relief: { maxSteps } }` | fields |
| `max_parallax_layer_count` | `maxParallaxLayerCount` | number | fields |
| `lightmap_exposure` | `lightmapExposure` | number | fields |
| `opaque_render_method` | `opaqueRenderMethod` | `"forward" \| "deferred" \| "auto"` | fields |
| `deferred_lighting_pass_id` | `deferredLightingPassId` | integer 0..255 | fields |
| `uv_transform` | `uvTransform` | `{ scale?, rotation?, translation? }` | fields |

A further test, `the_table_covers_standard_material`, walks `StandardMaterial`'s reflected field names and fails on any field neither in the table's prop map nor in a short list of fields named otherwise (`base_color`, `perceptual_roughness`, `alpha_mode`). A Bevy upgrade that adds a field then fails CI rather than going unnoticed.

## Risks / Trade-offs

- A `uvTransform` scale above 1 clamps instead of tiling: Bevy's default sampler clamps to edge, and sampler settings are an image concern no one owns yet. Offsets, rotation and sub-rects work. Follow-up with script-assets or a texture-settings change.
- One image loaded as both colour and data keeps its first colour space (decision 4).
- `doubleSided` alone does not turn off culling; Bevy wants `cullMode={null}` too. Docs show both together rather than coupling them, which would differ from Bevy.
- The four features enlarge every `StandardMaterial` bind group. With all eight extra textures, a material binds 14 textures and 14 samplers before the view's shadow and environment maps; where Bevy cannot use bindless, the per-stage total may pass wgpu's default 16. Task 5 counts it; if it does, the change stops for lead@bevy-ts, as it does for a measurable frame cost.
- A tangent mesh is a second copy of a shape. An entity flipping its normal map on and off swaps handles each time; both stay cached.

## Migration Plan

Rebase onto main after material-alpha lands. No migration for authors: every new prop is optional and shipped props keep their meaning.
