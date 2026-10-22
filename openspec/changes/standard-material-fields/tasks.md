## 1. Before building

- [ ] 1.1 Rebase onto main once material-alpha lands

## 2. `<material>` fields (`crates/bevy-ts-tsx`)

- [ ] 2.1 `Cargo.toml`: turn on `pbr_transmission_textures`, `pbr_specular_textures`, `pbr_multi_layer_material_textures`, `pbr_anisotropy_texture`
- [ ] 2.2 `intrinsics.rs`: `MaterialProps` gains every prop in the design's coverage table; serde enums (`rename_all = "camelCase"`) for UV channel, `cullMode` (`Option`), `opaqueRenderMethod`, `parallaxMappingMethod`; a `UvTransform { scale, rotation, translation }` struct
- [ ] 2.3 `check()`: finite numbers, colours through `color()`, non-empty texture paths
- [ ] 2.4 `material()` sets each field; textures load through the `AssetServer`, sRGB for base colour, emissive and specular tint, `is_srgb: false` for the rest; no `AssetServer` with a texture set fails at the element
- [ ] 2.5 A texture that fails to load (`AssetLoadFailedEvent<Image>`) logs an error at its element through `reconcile::log_error`, naming the path
- [ ] 2.6 Lazy tangents: `IntrinsicMesh` (the entity's `<mesh>` props) and the `NeedsTangents` marker, set by `insert_material` for a normal map, clearcoat normal map or anisotropy and cleared otherwise, by `remove_material` and by a script material; `fit_mesh(world, e)` called by both sides inserts the plain or the tangent handle; `IntrinsicAssets` caches tangent meshes per shape, falling back to the plain handle when generation fails

## 3. Types (`packages/bevy-ts`)

- [ ] 3.1 `jsx-runtime.d.ts`: `MaterialProps` gains every prop with its type (`UvChannel`, `Face | null`, `ParallaxMappingMethod`, `OpaqueRendererMethod`, `UvTransform`)
- [ ] 3.2 `test/tsx/elements.tsx`: a material using every prop type-checks; `cullMode="both"` and `baseColorTexture={1}` are type errors

## 4. Tests and docs

- [ ] 4.1 `crates/bevy-ts-tsx/tests/standard_material.rs`: `every_prop_sets_its_field` (one case per table row), `every_texture_loads_its_path_in_its_colour_space`, `the_table_covers_standard_material` (walks the reflected fields), bad values fail at the element (`cullMode="both"`, `NaN`, bad `uvTransform`, empty path), equal props share one handle, a missing texture file reports its path; tangents: a plain mesh has none, a normal-mapped one has them, two normal-mapped cuboids share one tangent mesh beside a plain one (two cuboid meshes built), removing the normal map restores the plain mesh, mesh inserted after material and material after mesh both fit
- [ ] 4.2 `crates/bevy-ts-cli/tests/materials.rs`, headless render: `a_textured_cube_draws_the_image`, `a_two_sided_plane_draws_its_back`, `a_normal_map_shades_a_cube`
- [ ] 4.3 `docs/materials.md`: a "Standard materials" section: every prop, texture colour spaces, `doubleSided` with `cullMode={null}`, `uvTransform` clamps rather than tiles
- [ ] 4.4 The full gate: build, workspace tests, clippy, `scripts/gen-bevy-types.sh --check`

## 5. Before landing: the texture-feature gate

- [ ] 5.1 `bench/standard-material-features/`: the arena plus 100 distinct `<material>`s (textured, normal-mapped), built with the four features on and off, 3 runs per arm, frame time and GPU time; results and the command in its `results/`
- [ ] 5.2 Count the `StandardMaterial` pipeline's sampled textures and samplers per shader stage, across all bind groups, against `wgpu::Limits::default()`; and create the pipeline headless on each of the 5 release platforms in CI (`release.yml`'s matrix)
- [ ] 5.3 If either shows a cost (a measurable frame or GPU time difference, or a limit passed), send the numbers to lead@bevy-ts and wait for its answer before landing
