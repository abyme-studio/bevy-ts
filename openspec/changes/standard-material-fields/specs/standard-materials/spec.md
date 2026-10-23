## ADDED Requirements

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
