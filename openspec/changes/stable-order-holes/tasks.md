## 1. Holes

- [x] 1.1 Hole 1: unbatchable bins' entities in an `IndexMap`, sorted by key under `StableBinOrder`; buffer indices follow (vendor `render_phase/mod.rs`, `batching/mod.rs`)
- [x] 1.2 Hole 2: `stable_keys` hashes the full affine, the local AABB, the asset path and the materials (`screenshot.rs`)
- [x] 1.3 Hole 3: `non_mesh_items` the same as unbatchable bins

## 2. Tests

- [x] 2.1 `bins_draw_in_key_order_with_stable_bin_order` covers unbatchable bins
- [x] 2.2 `one_mesh_turned_half_a_turn_follows_no_spawn_order`
- [x] 2.3 `two_materials_on_one_mesh_follow_no_spawn_order`
- [x] 2.4 The 10-run `a_gltf_race_gives_one_shot` still gives one shot

## 3. Docs

- [x] 3.1 `vendor/bevy_render/PATCH.md`, `docs/build.md`

## 4. Measure (after landing, engine-core's rule)

- [ ] 4.1 `bench/stable-bin-order`, patch vs main: median PhaseSort time under the fixed step within 2% or 0.1 ms
