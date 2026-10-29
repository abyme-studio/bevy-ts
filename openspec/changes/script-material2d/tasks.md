## 1. The erased 2D asset

- [ ] 1.1 Spike in `crates/bevy-ts-material/tests/it/material2d_render.rs` (a module in `tests/it/main.rs`, render app from `alpha_render::app_with` under its `GPU` lock): `ScriptMaterial2d` and `MeshScriptMaterial2d` in `crates/bevy-ts-material/src/material2d.rs`, `ErasedRenderAsset` returning `PreparedMaterial2d` (design 1), `DrawScriptMaterial2d` added to `Opaque2d`, `AlphaMask2d` and `Transparent2d`, the extract and specialisation systems written for `MeshScriptMaterial2d`; a fragment returning red turns a 64×64 `Mesh2d` rectangle red in a 320×180 `Camera2d` image, and inserting a new shader at its handle redraws without a restart. If Bevy's `specialize_material2d_meshes` or `queue_material2d_meshes` skips the entity, or `SetMaterial2dBindGroup` finds no allocator, stop and report before going on
- [ ] 1.2 `material2d.rs`: the three alpha modes to their phases and `Mesh2dPipelineKey` bits; the image-sampler rebinding and the after-images dependency as `render.rs` has them; `ScriptMaterialRenderPlugin` (`src/lib.rs`) adds the 2D half and exports its public types

## 2. Shared pieces

- [ ] 2.1 `layout.rs`: `Bindings` takes a space (2D or 3D) that sets the group (2 or 3) and the `get_tag` import (`bevy_sprite_render::mesh2d::functions::get_tag` in 2D); unit tests for the 2D declarations and imports
- [ ] 2.2 `wgsl.rs`: check a 2D kind against Bevy's 2D modules with `Mesh2dPipeline`'s shader defs, from the live pipeline as 3D's are; errors map to the author's `file:line:col` unchanged
- [ ] 2.3 `kinds.rs`: a `space` field on `materials` entries; one namespace of names across 2D, 3D and passes; 2D kinds build `Kind`s and `ScriptMaterial2d` keyed assets, re-encode by name, wait on a failing shader, apply `.wgsl` saves through `files_changed`, report renderer failures through `PipelineWatch`
- [ ] 2.4 `instance.rs`: a row for an entity holding `ScriptMaterial3d` or `MeshScriptMaterial2d`, one buffer for both
- [ ] 2.5 `crates/bevy-ts-material/ts/material.js` and `material.d.ts`: `material2d(name, { params, instance, textures, fragment, vertex?, alphaMode? })` and `Material2dKind`; 2D alpha modes only; a WGSL string rejected with the declaring line

## 3. TSX

- [ ] 3.1 `crates/bevy-ts-tsx/src/intrinsics.rs` and `resolve.rs`: `<material2d kind>` beside a `<mesh2d>` inserts `MeshScriptMaterial2d` with the keyed asset and the entity's `ScriptInstance`; bound instance props write the row; a bound param or texture, a prop the kind lacks, a 3D kind, a 2D kind on `<material>`, or an entity holding `Sprite` fails at the element; without `kind` it stays `ColorMaterial`; removal removes the component
- [ ] 3.2 `packages/bevy-ts/jsx-runtime.d.ts` (and `jsx-dev-runtime.d.ts` if it lists intrinsics): `Material2dProps` with `kind`, typed from the kind's params, instance params (bindable) and textures

## 4. Tests (docs/testing.md)

- [ ] 4.1 `crates/bevy-ts-material/tests/it/material2d.rs` (no renderer; a module in `tests/it/main.rs`): `equal_props_share_one_asset_and_instance_rows_hold_their_own`, `a_3d_alpha_mode_or_a_shared_name_fails_at_material2d`, `a_schema_change_re_encodes_assets_and_rows_by_name`, `the_2d_declarations_sit_at_group_2_with_the_2d_get_tag`
- [ ] 4.2 `crates/bevy-ts-material/tests/it/material2d_render.rs` (renders): `a_red_kind_draws_a_red_rectangle`, `an_instance_param_reads_per_quad`, `world_space_art_pixels_are_whole_blocks_at_a_fractional_position`, `quad_space_art_pixels_are_whole_blocks_on_whole_units`, `the_lower_of_two_y_sorted_quads_draws_in_front`, `a_replaced_shader_draws_within_frames_and_a_bad_one_keeps_the_last`
- [ ] 4.3 `crates/bevy-ts-cli/tests/it/materials_2d.rs` (a module in its `main.rs`; render app from `common::render_app()`): `a_tsx_material2d_kind_draws_and_a_bound_instance_prop_writes_its_row`, `material2d_kind_refuses_a_sprite_a_3d_kind_and_a_bound_param_at_the_element`, `a_wgsl_save_redraws_with_no_new_generation`, `a_wgsl_error_names_the_kind_and_the_authors_line`
- [ ] 4.4 Run the `scoped` process with `TESTS` naming `material2d:: material2d_render:: materials_2d:: two_d:: two_d_doc:: materials:: pass::` (docs/check-slots.md, Scoped tests); no per-change gate entry

## 5. Bench (rule 8)

- [ ] 5.1 `bench/material2d/` (`Cargo.toml` outside the workspace with the `bevy_render` patch as `bench/fullscreen-pass` has, `src/main.rs`, `shaders/sea.wgsl`, `land.wgsl`, `cloud.wgsl`, `ring.wgsl` with an integer hash at the research's hashes per pixel, `run.sh` sourcing `scripts/slot.sh`): 1920×1080 image target, `PixelCamera` zoom 3, waiting for the GPU every frame; arms `off`, `one`, `n20`, `n100`, `n500` (design 8); median and p95 frame time and the 2D main pass span's CPU time over 600 frames after 120 warm-up, arms alternated; commit and `uptime` printed
- [ ] 5.2 `material2d-bench` (`${RUNS:-3}` runs) and `material2d-bench-8` in `.abyme/processes.ts`, heavy, appending to `bench/material2d/results/runs.txt`; run it; `bench/material2d/results/README.md` with machine, load, commit, the table per arm and the verdict against the gate and the research estimate

## 6. Docs and example

- [ ] 6.1 `docs/materials.md`: "2D materials": declaring with `material2d()`, `<material2d kind>`, the appended WGSL at group 2 and the 2D imports, alpha modes, art pixels (quad space and world space, with the placement rule), Y-sort and layer z, batching and z bands, hot reload and errors, the measured cost from 5.2; its intro names 2D materials; "Not yet" drops script `Material2d` and names script sprite materials
- [ ] 6.2 `docs/2d.md`: the `<material2d>` section points at materials.md for `kind`; the coverage table gets a working row (`Material2d`, custom, `material2d()` and `<material2d kind>`, the 4.2 and 4.3 tests) that `two_d_doc` checks, and the "not planned" row keeps `Wireframe2d` and `SpriteMesh` and adds script sprite materials as a follow-up
- [ ] 6.3 `examples/shapes-2d`: `src/planet.wgsl` and a two-layer planet (`<mesh2d shape="rectangle">` with a sea kind and a land kind) under its camera; repo-made art only
- [ ] 6.4 `CHANGELOG.md`: one line under Unreleased
