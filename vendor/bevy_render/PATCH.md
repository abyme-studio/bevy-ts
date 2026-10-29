# bevy-ts's patch to bevy_render

This is bevy_render 0.20.0 as published on crates.io, with two changes. The workspace `Cargo.toml`
and every generated project engine (`crates/bevy-ts-cli/src/engine.rs`, `Source::patch`) use it
through `[patch.crates-io]`.

## Async pipelines on macOS

`src/render_resource/pipeline_cache.rs`, lines 1058-1090, the two `create_pipeline_task`s: the
first cfg loses `not(target_os = "macos")`, the second loses `target_os = "macos"`. So macOS spawns
each pipeline's compile on `AsyncComputeTaskPool`, as other multi-threaded desktop targets do,
instead of `block_on` on the render thread. A caller that asks for a synchronous compile still
gets one.

`diff -r ~/.cargo/registry/src/*/bevy_render-0.20.0/src src` shows those two lines and the
stable bin order below, nothing else.

## Why

On macOS every pipeline compiled on the render thread, one after another, so a fresh engine froze
on its first render until all of them were done. Reported upstream as bevy#26151. The arena,
headless on an M5 Max, before and after:

Each cell: time from engine start to all 46 pipelines ready / longest render frame meanwhile (load average at the run).

| Arm | Run 1 | Run 2 | Run 3 |
|---|---|---|---|
| before, cold | 10.95 s / 7.74 s (38) | 11.25 s / 7.73 s (37) | 14.60 s / 10.97 s (41) |
| before, warm | 53.75 s / 43.70 s (94) | 4.58 s / 2.58 s (76) | 2.44 s / 1.41 s (68) |
| after, cold | 4.33 s / 95 ms (22) | 4.05 s / 107 ms (23) | 4.59 s / 103 ms (28) |
| after, warm | 4.59 s / 120 ms (29) | 1.60 s / 283 ms (27) | 1.60 s / 409 ms (27) |

The method is in `openspec/changes/archive/*-macos-async-pipelines/design.md`, Measurements.

## Stable bin order

`src/render_phase/mod.rs` adds the resource `StableBinOrder(MainEntityHashMap<u64>)`, a key per
main entity, with `StableBinOrder::order`, and `RenderBin::sort`. While the resource is there:

- `src/batching/mod.rs`'s `sort_binned_render_phase` sorts each bin's entities by key, then
  `MainEntity` (only when they are not sorted already), and orders `batchable_meshes`,
  `unbatchable_meshes` and `non_mesh_items` by each bin's first key, then the bin's own key (a
  stable sort after Bevy's). That covers `Opaque3d`, `AlphaMask3d` and their prepasses.
- `UnbatchableBinnedEntities::entities` and `NonMeshEntities::entities` are an
  `IndexMap<MainEntity, Entity, EntityHash>`, not a `MainEntityHashMap`, so they have an order to
  sort; a removal is a `swap_remove`. Without the resource, they draw in the order they were
  queued, not in the hash order of their ids. An unbatchable bin's `buffer_indices` are given out
  by iterating its entities after `PhaseSort`, and drawing iterates them in the same order, so
  the indices stay matched.
- `src/render_phase/mod.rs`'s `sort_phase_system` orders a sorted phase's keyed items by key, then
  `MainEntity`, before `SortedPhaseItem::sort`; items with no key keep their order, after them.
  `Transparent2d` and `Transparent3d` sort with Bevy's stable `sort_by_key`, so items at one depth
  keep that order.

An entity with no key sorts after every keyed one. Without the resource nothing changes, so an
app that never inserts it pays nothing. `bevy-ts-remote`'s `ReadyPlugin` fills it under `FixedStep`
from its `StableKey`s: a hash of the whole transform, the local AABB, the mesh's or sprite image's
asset path, and a hash of each material's fields.

Why: under `NoIndirectDrawing` (which `FixedStep` gives every camera), with `depth_compare:
GreaterEqual`, the last of two draws at one depth wins the pixel. Bevy orders bins by
`(BatchSetKey, BinKey)`, all allocation ids (pipeline id, material slot, mesh `AssetId`), and a
glTF reserves its sub-assets' ids from its load task, so the bin order followed which glTF finished
loading first: agent town's civ shots flipped a few leaf pixels between runs (research doc
`leaf-flips.md`, team leaf-flips). Inside a bin, instances drew in the order they were queued,
which follows when each one's pipeline was ready: `stacked_instances_at_one_depth_are_the_same_every_shot`
failed on CI's rust job (352701 of 2073600 bytes).

`bins_draw_in_key_order_with_stable_bin_order` tests the patch on batchable and unbatchable bins;
`a_tie_at_one_depth_follows_no_load_order`, `a_gltf_race_gives_one_shot`,
`a_town_like_scene_is_the_same_every_shot`, `one_mesh_turned_half_a_turn_follows_no_spawn_order`
and `two_materials_on_one_mesh_follow_no_spawn_order` (all in `bevy-ts-cli`'s `screenshot_ready`)
test the shots.

## A Bevy bump

Cargo ignores a `[patch]` whose version no longer matches the lock, with only a warning, so the
fix would silently go. The `bevy-ts-cli` test `vendored_bevy_render_matches_the_locked_bevy` fails
instead. On a bump:

1. If the new Bevy compiles pipelines async on macOS (bevy#26151 fixed) and sorts a bin's
   entities, drop the patch (below); if only one, keep the other.
2. Otherwise copy the new crate from `~/.cargo/registry/src/*/bevy_render-<version>/`
   (`Cargo.toml`, `LICENSE-*`, `README.md`, `src/`) over this folder, keeping this file, commit it
   unedited, re-apply the two cfg edits and the stable bin order, and `cargo update -p bevy_render`.

## Dropping the patch

Delete `vendor/bevy_render`; remove `bevy_render` from the workspace `Cargo.toml`'s
`[patch.crates-io]` and `vendor/bevy_render` from its `exclude`; remove `Source::patch` and the
`[patch.crates-io]` lines from `Plan::manifest()` in `crates/bevy-ts-cli/src/engine.rs` with their
tests; bump `GENERATOR`; `cargo update -p bevy_render`; drop the notes in `docs/native.md`,
`docs/build.md` and `docs/release.md`.
