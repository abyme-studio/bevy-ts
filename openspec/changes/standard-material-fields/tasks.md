## 1. Before building

- [ ] 1.1 Operator's go on the two items in the proposal (Bevy PBR texture features; tangents on `<mesh>`). Without the first, drop the "feature" rows of the coverage table from every task below
- [ ] 1.2 Rebase onto main once material-alpha lands

## 2. `<material>` fields (`crates/bevy-ts-tsx`)

- [ ] 2.1 `Cargo.toml`: turn on `pbr_transmission_textures`, `pbr_specular_textures`, `pbr_multi_layer_material_textures`, `pbr_anisotropy_texture`
- [ ] 2.2 `intrinsics.rs`: `MaterialProps` gains every prop in the design's coverage table; serde enums (`rename_all = "camelCase"`) for UV channel, `cullMode` (`Option`), `opaqueRenderMethod`, `parallaxMappingMethod`; a `UvTransform { scale, rotation, translation }` struct
- [ ] 2.3 `check()`: finite numbers, colours through `color()`, non-empty texture paths
- [ ] 2.4 `material()` sets each field; textures load through the `AssetServer`, sRGB for base colour, emissive and specular tint, `is_srgb: false` for the rest; no `AssetServer` with a texture set fails at the element
- [ ] 2.5 A texture that fails to load (`AssetLoadFailedEvent<Image>`) logs an error at its element through `reconcile::log_error`, naming the path
- [ ] 2.6 `mesh_handle`: `with_generated_tangents()` on each built mesh, keeping the mesh as built when it fails

## 3. Types (`packages/bevy-ts`)

- [ ] 3.1 `jsx-runtime.d.ts`: `MaterialProps` gains every prop with its type (`UvChannel`, `Face | null`, `ParallaxMappingMethod`, `OpaqueRendererMethod`, `UvTransform`)
- [ ] 3.2 `test/tsx/elements.tsx`: a material using every prop type-checks; `cullMode="both"` and `baseColorTexture={1}` are type errors

## 4. Tests and docs

- [ ] 4.1 `crates/bevy-ts-tsx/tests/standard_material.rs`: `every_prop_sets_its_field` (one case per table row), `every_texture_loads_its_path_in_its_colour_space`, `the_table_covers_standard_material` (walks the reflected fields), bad values fail at the element (`cullMode="both"`, `NaN`, bad `uvTransform`, empty path), equal props share one handle, a missing texture file reports its path, `<mesh>` meshes carry tangents
- [ ] 4.2 `crates/bevy-ts-cli/tests/materials.rs`, headless render: `a_textured_cube_draws_the_image`, `a_two_sided_plane_draws_its_back`, `a_normal_map_shades_a_cube`
- [ ] 4.3 `docs/materials.md`: a "Standard materials" section: every prop, texture colour spaces, `doubleSided` with `cullMode={null}`, `uvTransform` clamps rather than tiles
- [ ] 4.4 The full gate: build, workspace tests, clippy, `scripts/gen-bevy-types.sh --check`
