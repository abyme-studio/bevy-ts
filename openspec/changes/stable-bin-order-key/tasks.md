## 1. Key in the main world

- [x] 1.1 `crates/bevy-ts-remote/src/screenshot.rs`: `StableKey(u64)` and the `PostUpdate` system that sets it under `FixedStep` (design 1)
- [x] 1.2 Same file: `stable_bin_order` extracts the keys into `StableBinOrder(MainEntityHashMap<u64>)`

## 2. Vendored bevy_render

- [x] 2.1 `vendor/bevy_render/src/render_phase/mod.rs`: `StableBinOrder(MainEntityHashMap<u64>)`; `RenderBin::sort` by key then `MainEntity`
- [x] 2.2 `vendor/bevy_render/src/batching/mod.rs`: `sort_binned_render_phase` orders `batchable_meshes` and `unbatchable_meshes` by smallest key, then bin key
- [x] 2.3 Check the order of entities inside an unbatchable bin; record it in PATCH.md
- [x] 2.4 `vendor/bevy_render/src/render_phase/mod.rs`: `sort_phase_system` orders items by key before `phase.sort()`
- [x] 2.5 `vendor/bevy_render/PATCH.md`, "Stable bin order": what changed, why, the tests; the `diff -r` line still lists every change

## 3. Tests (`crates/bevy-ts-cli/tests/it/screenshot_ready.rs`, per docs/testing.md)

- [x] 3.1 `a_tie_at_one_depth_follows_no_load_order`: two overlapping quads, red and blue, in one plane; shots with the mesh asset order flipped and with the glTF read order flipped are byte-identical (the two repro tests as one)
- [x] 3.2 `a_gltf_race_gives_one_shot`: 10 shots with both reads released at once, one distinct shot
- [x] 3.3 `a_town_like_scene_is_the_same_every_shot`: three tree glTFs sharing one alpha-mask leaf material with foliage overlapping at one depth, plus transparent sprites at one z; 10 shots, reads released at once, half with 30 ms of extra work a frame; one distinct shot
- [x] 3.4 Rewrite `a_bin_draws_in_entity_order_with_stable_bin_order` as `bins_draw_in_key_order_with_stable_bin_order`: two bins and scrambled keys; with `StableBinOrder` bins and entities follow the keys, without it Bevy's order
- [ ] 3.5 Run with `~/dev/abyme/scripts/gate-slot run … cargo test -p bevy-ts-cli --test it screenshot_ready` and confirm 3.1–3.3 fail on main's vendor (the test would have caught it)

## 4. Docs

- [x] 4.1 `docs/build.md`: the `StableBinOrder` sentence says bins and sprites at one depth, keyed by place, not only a batch's instances

## 5. Measure (before landing)

- [ ] 5.1 A civ-like frame (the 3.3 scene scaled to agent town's civ counts, or agent town's civ-hazards scene): shot time and the `sort_binned_render_phase` span, with and without the patch, three runs each, load noted; results in design.md "Measurements" and the team note
- [ ] 5.2 Acceptance in agent town: civ-hazards and civ-meaning give 1 hash in 10 at load 60–150
