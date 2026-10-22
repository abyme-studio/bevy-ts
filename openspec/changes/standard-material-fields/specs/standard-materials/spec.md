## ADDED Requirements

### Requirement: A standard material takes every StandardMaterial field
`<material>` without `kind` SHALL take a prop for every field of Bevy 0.20's `StandardMaterial`, named as the field in camelCase, except the fields it already names otherwise (`color` for `base_color`, `roughness` for `perceptual_roughness`) and `alpha_mode` (material-alpha's `alphaMode`). An unset prop SHALL leave the field at `StandardMaterial::default()`. Numbers SHALL be finite. Colours (`specularTint`, `attenuationColor`) SHALL take the forms `color` takes. Enum props SHALL take lowercase camelCase names, as `alphaMode` does: `cullMode` is `"back"`, `"front"` or `null` (no culling); a UV channel prop is `"uv0"` or `"uv1"`; `opaqueRenderMethod` is `"forward"`, `"deferred"` or `"auto"`; `parallaxMappingMethod` is `"occlusion"` or `{ relief: { maxSteps } }`. `uvTransform` SHALL be `{ scale?: [x, y], rotation?: radians, translation?: [x, y] }`, built as `Affine2::from_scale_angle_translation` with unset parts at identity. An unknown prop, a non-finite number, a bad colour, an unknown enum name or a malformed `uvTransform` SHALL fail the load at the element. Equal props SHALL share one material, as every `<material>` does.

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

### Requirement: A texture is an asset path
Every `Option<Handle<Image>>` field SHALL be a prop taking an asset path, loaded by the `AssetServer`. `baseColorTexture`, `emissiveTexture` and `specularTintTexture` SHALL load as sRGB; every other texture SHALL load as linear data (`is_srgb: false`), as Bevy's glTF loader does. An empty path SHALL fail the load at the element, and so SHALL a texture with no `AssetServer`. A path that fails to load SHALL be reported as an error at the element, naming the path. A texture file saved again SHALL redraw with no TS change, by Bevy's asset reload.

#### Scenario: A textured crate
- **WHEN** an entity has `<mesh shape="cuboid" />` and `<material baseColorTexture="textures/crate.png" />`
- **THEN** its `base_color_texture` is the handle for `textures/crate.png`, and once loaded the cube draws the image

#### Scenario: A normal map loads linear
- **WHEN** a material has `normalMapTexture="textures/wall_n.png"`
- **THEN** the image loads with `is_srgb: false`

#### Scenario: A missing file
- **WHEN** a material has `baseColorTexture="textures/nope.png"` and no such file exists
- **THEN** an error at that element names `textures/nope.png`

### Requirement: A normal map shades a mesh shape
Every mesh `<mesh>` builds SHALL carry vertex tangents, so `normalMapTexture` and anisotropy shade a `<mesh>` shape. A shape whose tangents cannot be generated SHALL be built without them, with no error.

#### Scenario: A normal-mapped cube
- **WHEN** an entity has `<mesh shape="cuboid" />` and `<material normalMapTexture="textures/bumps.png" />`
- **THEN** the cube's mesh has `Mesh::ATTRIBUTE_TANGENT`, and its lit faces differ from the same cube without the normal map
