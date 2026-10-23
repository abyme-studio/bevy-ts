## ADDED Requirements

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
