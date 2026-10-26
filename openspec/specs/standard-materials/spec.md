# standard-materials Specification

## Purpose
`<material>` without `kind` builds Bevy's `StandardMaterial` from its props, and `MaterialAlpha` fades the standard materials an entity draws, on a mesh or a glTF scene root, without editing the shared originals.

## Requirements

### Requirement: A standard material takes alpha and an alpha mode
`<material>` without `kind` SHALL build a `StandardMaterial` from `color`, `metallic`, `roughness`, `emissive`, `unlit`, `alpha` and `alphaMode`. `alpha` SHALL be a number from 0 to 1 that replaces the alpha of `base_color`; a colour written with alpha (`"#ff000080"`) SHALL keep its own alpha when `alpha` is unset. `alphaMode` SHALL take the values a script material's `alphaMode` takes: `"opaque"`, `"blend"`, `"premultiplied"`, `"add"`, `"multiply"`, `"alphaToCoverage"` or `{ mask: cutoff }`. Unset, it SHALL be `Blend` when the final alpha is below 1 and `Opaque` otherwise, as Bevy's `StandardMaterial::from(Color)`. An `alpha` outside 0 to 1, or an unknown `alphaMode`, SHALL fail the load at the element. Equal props SHALL share one material, as every `<material>` does.

#### Scenario: A half see-through cube
- **WHEN** an entity has `<mesh shape="cuboid" />` and `<material color="#ff0000" alpha={0.5} />`
- **THEN** its `StandardMaterial` has `base_color` alpha 0.5 and `AlphaMode::Blend`, and the cube is drawn blended over what is behind it

#### Scenario: An explicit mode wins
- **WHEN** a material has `alpha={0.5}` and `alphaMode={{ mask: 0.3 }}`
- **THEN** its mode is `AlphaMode::Mask(0.3)`

#### Scenario: A bad alpha
- **WHEN** a material has `alpha={2}`
- **THEN** the load fails at that element, saying alpha takes 0 to 1

### Requirement: MaterialAlpha fades an entity's materials
An entity with `MaterialAlpha { alpha }` SHALL fade its targets: the entity itself when it holds a `MeshMaterial3d<StandardMaterial>`, and, when it has `WorldAssetRoot`, every mesh its scene instance spawned that holds one, once the instance is ready. Each target SHALL draw with a `FadeMaterial` copy (`ExtendedMaterial<StandardMaterial, Fade>`) owned by the entity: one copy per distinct original material per entity, its base a clone of the original. A copy of an `Opaque`, `Mask` or `AlphaToCoverage` original SHALL keep the original's mode and `base_color`, and SHALL discard each pixel whose threshold on a 4×4 ordered (Bayer) screen-space map is above `alpha` (clamped to 0..1), so about `alpha` of the pixels draw, in steps of 1/16, with depth written. A copy of a `Blend`, `Premultiplied`, `Add` or `Multiply` original SHALL keep its mode, SHALL have the original's `base_color` alpha times `alpha`, and SHALL NOT discard in the main pass. At 1 a copy SHALL draw as the original; at 0 it SHALL draw nothing. Originals SHALL never be edited, so another entity sharing the material is not faded. A changed `alpha` SHALL reach every copy the frame it changes by editing the copies in place, creating no asset; the sync SHALL run after Bevy's `AnimationSystems`, so an `alpha` an animation writes reaches the copies that frame. A field binding (`<MaterialAlpha alpha={() => k()} />`) SHALL drive it each frame with no remount. A target given another `StandardMaterial` while faded SHALL be faded again from that material. Removing `MaterialAlpha` SHALL give each target its original `MeshMaterial3d<StandardMaterial>` back, remove its `MeshMaterial3d<FadeMaterial>`, and drop the copies. A scene spawned again (ready again) SHALL be faded again, and an original that reloads SHALL be copied again. TSX children of the entity, entities its scene did not spawn, and meshes drawing a script material SHALL be left alone. `alpha.rs` SHALL be the one place that knows a faded mesh's original; other code (scene-material) SHALL ask it.

#### Scenario: A shape fades out
- **WHEN** an entity has `<mesh shape="cuboid" />`, `<material color="#ff0000" />` and `<MaterialAlpha alpha={() => k()} />`, and `k` goes from 1 to 0 over 2 s
- **THEN** each frame the cube draws with the entity's `FadeMaterial` copy at alpha `k`, its mode `Opaque`, about `k` of its pixels opaque red and the rest showing what is behind, and no material asset is added after the first frame

#### Scenario: A model fades as one solid
- **WHEN** a red cube holds a smaller black cube inside it, both under one faded root at `alpha` 0.5, drawn unlit over white with `Msaa::Off`
- **THEN** no pixel in the red cube's area is near black, and the fraction of red pixels there is 0.5 ± 1/16

#### Scenario: An already transparent material fades
- **WHEN** a faded entity's original is `<material color="#ff0000" alpha={0.5} />` (`Blend`) and `alpha` is 0.5
- **THEN** its copy is `Blend` with `base_color` alpha 0.25, and its main pass discards nothing

#### Scenario: Another shape with the same material
- **WHEN** a second entity has the same `<mesh>` and `<material color="#ff0000" />` and no `MaterialAlpha`, while the first fades
- **THEN** the second cube draws with the shared material, opaque red

#### Scenario: A shape's material changes while faded
- **WHEN** a TS save changes the faded cube's `<material color>` to `"#0000ff"`
- **THEN** the cube draws blue at alpha `k`, and removing `MaterialAlpha` leaves it opaque blue

#### Scenario: A cluster fades out
- **WHEN** ten entities each have `WorldAssetRoot("models/house.glb#Scene0")` and `<MaterialAlpha alpha={() => k()} />`, and `k` goes from 1 to 0 over 2 s
- **THEN** each frame every house mesh draws with its root's `FadeMaterial` copy at alpha `k`, and no material asset is added after the first frame

#### Scenario: Another instance of the same model
- **WHEN** a second root spawns the same model with no `MaterialAlpha`, while the first fades
- **THEN** the second root's meshes draw with the model's own materials, unchanged

#### Scenario: Removing it
- **WHEN** `MaterialAlpha` is removed from a faded shape or root
- **THEN** every target holds its own `MeshMaterial3d<StandardMaterial>` again and no `MeshMaterial3d<FadeMaterial>`, and the entity's copies are dropped

#### Scenario: A hull tinted by a script material
- **WHEN** a faded root also has a script material that swapped its `hull` meshes (scene-material)
- **THEN** the `hull` meshes keep the script material, unfaded, and the other meshes fade; removing either component leaves each mesh with what the other still asks for

### Requirement: A standard material takes every StandardMaterial field
`<material>` without `kind` SHALL take a prop for every field of Bevy 0.20's `StandardMaterial` that Bevy compiles with its default features, named as the field in camelCase, except the fields it already names otherwise (`color` for `base_color`, `roughness` for `perceptual_roughness`) and `alpha_mode` (material-alpha's `alphaMode`). The textures and UV channels behind Bevy's `pbr_transmission_textures`, `pbr_specular_textures`, `pbr_multi_layer_material_textures` and `pbr_anisotropy_texture` features SHALL NOT be props, and those features SHALL stay off: with them on, Bevy's opaque pipeline binds more sampled textures and samplers than `wgpu::Limits::default()` allows. An unset prop SHALL leave the field at `StandardMaterial::default()`. Numbers SHALL be finite. Colours (`specularTint`, `attenuationColor`) SHALL take the forms `color` takes. Enum props SHALL take lowercase camelCase names, as `alphaMode` does: `cullMode` is `"back"`, `"front"` or `null` (no culling); a UV channel prop is `"uv0"` or `"uv1"`; `opaqueRenderMethod` is `"forward"`, `"deferred"` or `"auto"`; `parallaxMappingMethod` is `"occlusion"` or `{ relief: { maxSteps } }`. `uvTransform` SHALL be `{ scale?: [x, y], rotation?: radians, translation?: [x, y] }`, built as `Affine2::from_scale_angle_translation` with unset parts at identity. An unknown prop, a non-finite number, a bad colour, an unknown enum name or a malformed `uvTransform` SHALL fail the load at the element. Equal props SHALL share one material, as every `<material>` does.

#### Scenario: A two-sided leaf
- **WHEN** a material has `doubleSided` and `cullMode={null}`
- **THEN** its `StandardMaterial` has `double_sided: true` and `cull_mode: None`, and the plane's back face is drawn lit

#### Scenario: Glass
- **WHEN** a material has `specularTransmission={0.9}`, `thickness={0.2}`, `ior={1.5}` and `attenuationColor="#88ccff"`
- **THEN** those four fields hold those values, the colour as sRGB `#88ccff`

#### Scenario: Relief parallax
- **WHEN** a material has `parallaxMappingMethod={{ relief: { maxSteps: 8 } }}`
- **THEN** its `parallax_mapping_method` is `ParallaxMappingMethod::Relief { max_steps: 8 }`

#### Scenario: A UV transform
- **WHEN** a material has `uvTransform={{ scale: [0.5, 0.5], translation: [0.5, 0] }}`
- **THEN** its `uv_transform` is `Affine2::from_scale_angle_translation(vec2(0.5, 0.5), 0.0, vec2(0.5, 0.0))`

#### Scenario: An unknown cull mode
- **WHEN** a material has `cullMode="both"`
- **THEN** the load fails at that element, naming `"back"`, `"front"` and `null`

#### Scenario: Two equal materials
- **WHEN** two entities have `<material color="#fff" baseColorTexture="t/crate.png" doubleSided />`
- **THEN** both hold the same `StandardMaterial` handle

#### Scenario: A blocked texture
- **WHEN** a material has `clearcoatTexture="textures/coat.png"`
- **THEN** the load fails at that element as an unknown prop

#### Scenario: The pipeline fits the default limits
- **WHEN** a cube's material sets every texture prop, on a device held to `wgpu::Limits::default()`
- **THEN** its opaque and prepass pipelines are created

### Requirement: A texture is an asset path
Every `Option<Handle<Image>>` field that is a prop SHALL take an asset path, loaded by the `AssetServer`. `baseColorTexture` and `emissiveTexture` SHALL load as sRGB; every other texture SHALL load as linear data (`is_srgb: false`), as Bevy's glTF loader does. An empty path SHALL fail the load at the element, and so SHALL a texture with no `AssetServer`. A path that fails to load SHALL be reported as an error at the element, naming the path. A texture file saved again SHALL redraw with no TS change, by Bevy's asset reload.

#### Scenario: A textured crate
- **WHEN** an entity has `<mesh shape="cuboid" />` and `<material baseColorTexture="textures/crate.png" />`
- **THEN** its `base_color_texture` is the handle for `textures/crate.png`, and once loaded the cube draws the image

#### Scenario: A normal map loads linear
- **WHEN** a material has `normalMapTexture="textures/wall_n.png"`
- **THEN** the image loads with `is_srgb: false`

#### Scenario: A missing file
- **WHEN** a material has `baseColorTexture="textures/nope.png"` and no such file exists
- **THEN** an error at that element names `textures/nope.png`

### Requirement: A mesh shape gets tangents when its material needs them
A `<mesh>` SHALL carry vertex tangents exactly when the `<material>` on its entity sets `normalMapTexture` or a non-zero `anisotropyStrength`; Bevy ignores those on a mesh without tangents. The tangent mesh SHALL be built once per shape (per `<mesh>` props) and shared by every entity with that shape and such a material. A `<mesh>` whose material does not need them SHALL keep the plain shared mesh, with no tangent attribute. The swap SHALL follow either element changing: a material that gains a normal map moves its entity to the tangent mesh, one that loses it moves back, and a `<mesh>` inserted after its material gets the right variant. A shape whose tangents cannot be generated SHALL keep its plain mesh, with no error.

#### Scenario: A plain mesh has no tangents
- **WHEN** an entity has `<mesh shape="cuboid" />` and `<material color="#f00" />`
- **THEN** its mesh has no `Mesh::ATTRIBUTE_TANGENT`

#### Scenario: A normal-mapped cube
- **WHEN** an entity has `<mesh shape="cuboid" />` and `<material normalMapTexture="textures/bumps.png" />`
- **THEN** its mesh has `Mesh::ATTRIBUTE_TANGENT`, and its lit faces differ from the same cube without the normal map

#### Scenario: Two entities share one tangent mesh
- **WHEN** two entities have `<mesh shape="cuboid" />` and normal-mapped materials, and a third has `<mesh shape="cuboid" />` with a plain material
- **THEN** the first two hold the same tangent `Mesh` handle, the third holds the plain one, and two cuboid meshes were built in all

#### Scenario: The normal map is removed
- **WHEN** a TS save removes `normalMapTexture` from a normal-mapped cube's material
- **THEN** the cube holds the plain cuboid mesh again

### Requirement: A faded target's shadow follows its alpha
A target drawing a `FadeMaterial` copy SHALL cast a shadow dithered by the same 4×4 map at the same `alpha`, whatever the original's alpha mode: at 1 the original's full shadow, at 0 none, between them a shadow that lightens as `alpha` falls. The dither SHALL run in the copy's prepass fragment shader, which the shadow pass uses. `MaterialAlpha` SHALL NOT add, remove or read `NotShadowCaster`; one already on a target SHALL keep it from casting at any `alpha`.

#### Scenario: Three cubes over a lit ground
- **WHEN** three cubes stand above a white ground plane under a directional light with shadows, at `MaterialAlpha` 1, 0.5 and 0
- **THEN** the ground under the 0.5 cube is lighter than under the 1 cube and darker than under the 0 cube, and the ground under the 0 cube equals unshadowed ground

#### Scenario: A faded scene's shadow
- **WHEN** a `ship.gltf` root at `MaterialAlpha` 0 stands over the lit ground
- **THEN** the ground under it equals unshadowed ground

#### Scenario: An authored NotShadowCaster stays
- **WHEN** a cube with `NotShadowCaster` gets `MaterialAlpha` 1, and the component is then removed
- **THEN** it casts no shadow throughout, and still holds `NotShadowCaster` after

### Requirement: A standard material is written through the asset path
`<material>` without `kind` SHALL build its `StandardMaterial` as the asset value `StandardMaterial.of` of its props, through the shared reflected-asset path, and share it with every element and `Handle<StandardMaterial>` slot given the same value. Each prop SHALL map by one table onto a `StandardMaterial` field: its name, value spellings, checks and error messages SHALL stay as standard-material-fields and material-alpha specify, including `alpha` and `alphaMode` and the unset `alphaMode` rule. `<mesh color>` SHALL share the same path. The `MaterialAlpha` component SHALL keep its behaviour. The props `MaterialProps` types in `bevy-ts/jsx-runtime` SHALL match the table, checked by a test.

#### Scenario: Same element, same field
- **WHEN** a material has `color="#f80"`, `roughness={0.3}` and `cullMode={null}`
- **THEN** its `StandardMaterial` has that `base_color`, `perceptual_roughness: 0.3` and `cull_mode: None`, as before this change

#### Scenario: An element and a value share
- **WHEN** one entity has `<material color="#fff" />` and another `<MeshMaterial3d value={StandardMaterial.of({ base_color: "#fff" })} />`
- **THEN** both hold the same handle

#### Scenario: Alpha folds in
- **WHEN** a material has `color="#ff0000"` and `alpha={0.5}`
- **THEN** its `base_color` alpha is 0.5 and its `alpha_mode` is `Blend`

### Requirement: A texture prop takes an asset value
A texture prop of `<material>` SHALL take an asset path, an `AssetRef<Image>` or an `Image.load(path, settings)` value. A path SHALL load with the colour space its field takes (sRGB for colour textures, linear for data textures). An `Image.load` value SHALL load with its own settings, its `is_srgb` defaulting to the field's colour space.

#### Scenario: A tiled texture
- **WHEN** a material has `baseColorTexture={Image.load("textures/atlas.png", { sampler: { Descriptor: { address_mode_u: "Repeat", address_mode_v: "Repeat" } } })}` and `uvTransform={{ scale: [4, 4] }}`
- **THEN** the cube draws the atlas repeated four times on each axis, not clamped

#### Scenario: A normal map given as a value keeps linear
- **WHEN** a material has `normalMapTexture={Image.load("textures/wall_n.png", {})}`
- **THEN** the image loads with `is_srgb: false`
