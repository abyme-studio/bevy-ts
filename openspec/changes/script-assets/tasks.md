## 0. Before starting

- [ ] 0.1 The operator's go on design "Ask-first items" 1 (the `AssetRef` host kind, strong refs freed on drop and reload) and 2 (remesh performance)
- [ ] 0.2 writable-native-params and reflect-assets stage 1 are on main; rebase. Agree with lead@reflect-assets: `ScriptAssets` stores an `UntypedHandle` per ref, and this change owns handle lifecycle and load state (their design names engine-core)
- [ ] 0.3 Cargo only through `~/dev/abyme/scripts/gate-slot`, none during a load freeze; nothing on the operator's screen (headless engines only)

## 1. Stage 1: handles and load state

- [ ] 1.1 `crates/bevy-ts/src/host.rs`: `Kind::AssetRef(type_path)` and `ScriptRef<A>(u32)`; JS glue wraps and unwraps `{ $asset: n }`, checks the asset type, throws naming method and both types; `bevy-ts types` declares `AssetRef<A>`; a test with a fixture export taking and returning `ScriptRef<Mesh>`
- [ ] 1.2 `crates/bevy-ts/src/reflect_asset.rs`: `insert_strong`, `drop`, `remove`; hot reload drops the old session's strong entries; stop drops and removes all
- [ ] 1.3 `crates/bevy-ts-cli/src/assets.rs`: `ScriptAssetServer` (`stands_for = "Res<AssetServer>"`) with `load(path, type?)`, `loadState`, `dependencyLoadState`, `recursiveDependencyLoadState`, `isLoaded`, `isLoadedWithDirectDependencies`, `isLoadedWithDependencies`; the failed-state text through a `Uint8Array`; labelled path without a type throws; registered in every engine
- [ ] 1.4 Runtime (`crates/bevy-ts-v8/js/core.js`, `packages/bevy-ts/runtime/core.js`): `assets.drop`, `assets.remove`, `ref.id`, and `isAdded`/`isModified`/`isRemoved`/`isUnused`/`isLoadedWithDependencies` on `AssetEvent`; types in `packages/bevy-ts/index.d.ts` and the `bevy-ts/assets` d.ts
- [ ] 1.5 `crates/bevy-ts-cli/tests/script_assets.rs`: `a_texture_loads_and_reaches_loaded`, `a_missing_file_fails_with_bevys_error`, `a_gltf_waits_for_its_dependencies`, `a_labelled_path_needs_a_type`, `a_dropped_mesh_nothing_holds_is_freed`, `a_dropped_mesh_an_entity_draws_stays`, `remove_frees_at_once`, `a_stale_ref_throws`, `hot_reload_frees_what_only_the_old_script_held`, `a_uuid_ref_keeps_its_asset_after_drop`, `a_load_is_seen_as_an_event` (needs rust-events; `#[ignore]` with its name until it lands)
- [ ] 1.6 `tests/typescript.rs`: `Res<AssetServer>` resolves to the export; `load` typed by its class argument
- [ ] 1.7 `docs/assets.md`: handles, load state, drop and remove, ids and events; `docs/native.md`: the `AssetRef` kind

## 2. Stage 2: `ResMut<Assets<Mesh>>` and the `Mesh` API

- [ ] 2.1 `assets.rs`: `ScriptMeshes` (`stands_for = "ResMut<Assets<Mesh>>"`): per-attribute and indices calls over slices, a finish call running recorded ops with Bevy's `try_*` forms, `get`, `getMut` methods, `remove`; same-length copy reuses the `Vec`; one `Modified` per frame per mesh
- [ ] 2.2 Runtime: `Mesh.new`, `PrimitiveTopology`, `RenderAssetUsages`, `Mesh.ATTRIBUTE_*` (name, id, format, components, array kind), `Indices.U16/U32`, the JS-side recorder for unadded meshes; `types.rs` emits these on the generated `Mesh` class
- [ ] 2.3 `script_assets.rs`: `a_quad_matches_the_same_rust_calls`, `every_attribute_round_trips`, `computed_normals_match_rust` (each `compute_*`, `generate_tangents`, `duplicate_vertices`, `count_vertices`), `tangents_need_normals_and_uvs`, `a_wrong_array_throws`, `render_world_only_data_cannot_be_edited`, `a_mesh_update_is_seen_as_modified`, `a_chunk_remesh_is_six_host_calls`
- [ ] 2.4 `crates/bevy-ts-cli/tests/mesh_update_bench.rs` (ignored by default): arms (a)–(e) of design decision 5, 3 runs × 200 frames, median and p95 µs per remesh and ns per vertex; record the numbers here and in `docs/research/script-assets.md`; target (b) ≤ 1.5 × (a) + 5 µs. Report to the lead before docs recommend the path
- [ ] 2.5 `docs/assets.md`: building and updating meshes, the cost numbers, drop or reuse instead of add per edit

## 3. Stage 3: `<mesh>` vertex data and `Mesh` off the hole list

- [ ] 3.1 mesh-primitives is on main; rebase
- [ ] 3.2 `reflect_asset.rs`: `Mesh`'s whole-type entry (design decision 6): read, write, default; `assets.get`/`set`/`Mesh.of` on it; `a_mesh_reads_back_as_bevy_named_json`
- [ ] 3.3 Delete `("bevy_mesh::mesh::Mesh", ...)` from `reflect_value::UNCOVERED`; `crates/bevy-ts-cli/tests/reflect_coverage.rs` passes. If it fails because the check does not treat hand-mapped types as covered, ask lead@reflect-enums-color for that filter; do not edit the walker
- [ ] 3.4 `crates/bevy-ts-tsx/src/intrinsics.rs`: `MeshProps`' vertex-data member, picked by `positions`; load checks (lengths, index range, topology, Bevy's refusals) with the element's location; writes `Mesh.of` into `Mesh3d`; `jsx-runtime.d.ts` member
- [ ] 3.5 `crates/bevy-ts-tsx/tests/vertex_mesh.rs`: a quad from props equals Bevy's calls; mismatched lengths fail at load; equal props share one handle; `tests/typescript.rs`: `shape` with `positions` is a `tsc` error
- [ ] 3.6 `docs/research/bevy-coverage.md`: the "Assets and scenes" and "Render" rows this change closes point at their tests

## 4. Landing (each stage)

- [ ] 4.1 Touched crates' tests through gate-slot while working; `pnpm check` and the affected gate before `abyme call land add`
- [ ] 4.2 Every coverage-table row names a test that exists or an owner
- [ ] 4.3 `openspec validate script-assets --strict`
