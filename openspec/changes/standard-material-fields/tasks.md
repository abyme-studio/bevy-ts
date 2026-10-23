## 1. Before building

- [ ] 1.1 Rebase onto main once material-alpha lands

## 2. `<material>` fields (`crates/bevy-ts-tsx`)

- [ ] 2.1 `Cargo.toml`: the four Bevy texture features off again (design decision 8)
- [x] 2.2 `intrinsics.rs`: `MaterialProps` gains every prop in the design's coverage table; serde enums (`rename_all = "camelCase"`) for UV channel, `cullMode` (`Option`), `opaqueRenderMethod`, `parallaxMappingMethod`; a `UvTransform { scale, rotation, translation }` struct
- [x] 2.3 `check()`: finite numbers, colours through `color()`, non-empty texture paths
- [x] 2.4 `material()` sets each field; textures load through the `AssetServer`, sRGB for base colour, emissive and specular tint, `is_srgb: false` for the rest; no `AssetServer` with a texture set fails at the element
- [x] 2.5 A texture that fails to load (`AssetLoadFailedEvent<Image>`) logs an error at its element through `reconcile::log_error`, naming the path
- [x] 2.6 Lazy tangents: `IntrinsicMesh` (the entity's `<mesh>` props) and the `NeedsTangents` marker, set by `insert_material` for a normal map, clearcoat normal map or anisotropy and cleared otherwise, by `remove_material` and by a script material; `fit_mesh(world, e)` called by both sides inserts the plain or the tangent handle; `IntrinsicAssets` caches tangent meshes per shape, falling back to the plain handle when generation fails

## 3. Types (`packages/bevy-ts`)

- [x] 3.1 `jsx-runtime.d.ts`: `MaterialProps` gains every prop with its type (`UvChannel`, `Face | null`, `ParallaxMappingMethod`, `OpaqueRendererMethod`, `UvTransform`)
- [x] 3.2 `test/tsx/elements.tsx`: a material using every prop type-checks; `cullMode="both"` and `baseColorTexture={1}` are type errors

## 4. Tests and docs

- [x] 4.1 `crates/bevy-ts-tsx/tests/standard_material.rs`: `every_prop_sets_its_field` (one case per table row), `every_texture_loads_its_path_in_its_colour_space`, `the_table_covers_standard_material` (walks the reflected fields), bad values fail at the element (`cullMode="both"`, `NaN`, bad `uvTransform`, empty path), equal props share one handle, a missing texture file reports its path; tangents: a plain mesh has none, a normal-mapped one has them, two normal-mapped cuboids share one tangent mesh beside a plain one (two cuboid meshes built), removing the normal map restores the plain mesh, mesh inserted after material and material after mesh both fit
- [x] 4.2 `crates/bevy-ts-cli/tests/materials.rs`, headless render: `a_textured_cube_draws_the_image`, `a_two_sided_plane_draws_its_back`, `a_normal_map_shades_a_cube`
- [x] 4.3 `docs/materials.md`: a "Standard materials" section: every prop, texture colour spaces, `doubleSided` with `cullMode={null}`, `uvTransform` clamps rather than tiles
- [ ] 4.4 The full gate: build, workspace tests, clippy, `scripts/gen-bevy-types.sh --check`

## 5. Before landing: the texture-feature gate

- [x] 5.1 `bench/standard-material-features/`: the arena plus 100 distinct `<material>`s (textured, normal-mapped), built with the four features on and off, 3 runs per arm, frame time and GPU time; results and the command in its `results/`
- [x] 5.2 Count the pipeline's textures and samplers against `wgpu::Limits::default()`: 18 and 18 with the features on, past 16 (bench results). lead@bevy-ts dropped the features
- [ ] 5.3 `crates/bevy-ts-cli/tests/standard_material_limits.rs` asserts the opaque and prepass pipelines are created on a device held to the default limits, with every texture prop set
- [ ] 5.4 The 18 blocked props removed from `MaterialProps`, `material()`, the tangent trigger (`clearcoatNormalTexture`, `anisotropyTexture`), the texture colour-space list, `jsx-runtime.d.ts`, `elements.tsx` and the tests; `elements.tsx` checks a blocked prop is a type error
- [ ] 5.5 `docs/materials.md`: a note that the nine textures are blocked on Bevy, with the limit counts, the benchmark's no-cost result and the upstream issue link
- [ ] 5.6 Fill `UPSTREAM_ISSUE_URL` in `design.md` with the Bevy issue (found or drafted)
