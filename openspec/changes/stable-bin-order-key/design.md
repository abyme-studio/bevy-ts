## Context

Cause and evidence are in the research doc (`leaf-flips.md`, team leaf-flips). Short form: a bin
key in Bevy 0.20 is `(BatchSetKey, BinKey)`, all allocation ids; `sort_binned_render_phase` sorts
bins by it and `render` draws them in that order; the depth test is `GreaterEqual`, so the last of
two draws at one depth wins. glTF sub-asset handles are reserved from the loader's async task, so
their `AssetId`s follow load timing. Entity ids do too: a glTF scene spawns in the frame its load
finishes.

Today's patch (`StableBinOrder`, a unit resource) sorts a batchable bin's entities by
`MainEntity`. It fixed instances inside one bin, not bins against each other.

## Goals / Non-Goals

Goals: under `FixedStep`, the order of draws at one depth is the same on every run, for binned
phases (`Opaque3d`, `AlphaMask3d`, their prepasses) and sorted phases (`Transparent2d`,
`Transparent3d`). Normal play unchanged.

Non-goals: a deterministic order for two items with the same AABB, no asset path and the same
sort key (coincident procedural meshes): they fall back to `MainEntity`. Changing Bevy's order
outside `FixedStep`. Multidraw bins: `FixedStep` gives every camera `NoIndirectDrawing`, so
`multidrawable_meshes` is empty.

## Decisions

1. The key is computed in the main world, in `PostUpdate` after `VisibilitySystems::CalculateBounds`
   and transform propagation, for entities whose `GlobalTransform` or `Aabb` changed, as a
   component `StableKey(u64)` (bevy-ts-remote, `screenshot.rs`). The hash is `FixedHasher` over
   the `f32` bits of the world AABB centre, the half extents, then the mesh's or sprite image's
   `AssetPath` from `AssetServer::get_path` when there is one. A script places its entities on the
   fixed step, so the centre is the same bits every run. Two primitives of one glTF node have
   different AABBs or asset paths (`#Mesh0/Primitive1`), so they differ.
   Rejected: smallest `MainEntity` per bin (entity ids follow load timing); loading glTFs one at a
   time (slower shots, and procedural meshes share the id counter); strict `Greater` depth (still
   order-dependent).
2. `stable_bin_order` (the extract system) fills `StableBinOrder(MainEntityHashMap<u64>)` from
   `StableKey`s each frame while `FixedStep` exists, and removes it otherwise.
3. Binned phases: in `sort_binned_render_phase`, `RenderBin::sort` takes the keys and sorts its
   `IndexMap` by `(key, MainEntity)`, missing keys last. `batchable_meshes` is sorted by
   `(smallest key in the bin, (BatchSetKey, BinKey))`, `unbatchable_meshes` the same over
   `UnbatchableBinnedEntities::entities`. Inside an unbatchable bin, entities stay in Bevy's order:
   `buffer_indices` is tied to it (check in task 2.3 whether that order is entity hash order; if it
   is, note it as a known limit in PATCH.md).
4. Sorted phases: in `sort_phase_system`, before `phase.sort()`, sort `items` by key. Bevy's
   `Transparent2d` and `Transparent3d` sort with the stable `sort_by_key`, so ties keep the key
   order. Opaque 2D phases that use `sort_unstable` are not touched.
5. Where the cost goes: one hash per moved entity (main world), one map extraction, and an
   n log n sort per phase per frame. Draw calls are unchanged under `NoIndirectDrawing`; reordered
   bins may add pipeline or bind-group switches.

## Risks / Trade-offs

- A sort of all bins per frame in shots: measured in task 5 before landing.
- Moving bins apart from their batch set can split what would be adjacent draws; harmless under
  `NoIndirectDrawing`, where every bin is already its own draw.
- The patch grows; a Bevy bump re-applies it (PATCH.md, "A Bevy bump").

## Measurements

To be filled by task 5: civ-like frame, shot time and the `sort_binned_render_phase` span, with
and without the patch, three runs each, load average noted.
