## Why

The stable bin order from `stable-bin-order-key` has three holes (research doc `still-flips.md`).
None is shown to cause alpha.43's civ flip, which follows agent town's camera fit. Each can still
flip a fixed-step shot:

1. Inside an unbatchable bin, entities draw in `MainEntityHashMap` order, the hash of their ids.
   Every skinned mesh is unbatchable: agent town's villagers.
2. The key hashes only the world AABB and the mesh path. One mesh turned by θ and by θ + 180° at
   one place, or two materials on one mesh at one place, share a key and fall back to entity order.
3. `non_mesh_items` are not ordered by key at all.

## What Changes

- Vendored `bevy_render`:
  - `UnbatchableBinnedEntities::entities` and `NonMeshEntities::entities` become an
    `IndexMap<MainEntity, Entity, EntityHash>`.
  - While `StableBinOrder` is there, `sort_binned_render_phase` sorts their entities by key, and
    their bins by first key.
  - The unbatchable buffer indices are given out after the sort, in the order drawing iterates.
- `StableKey` hashes the whole transform, the local AABB, the mesh or image path, and each
  material's reflected fields (`MeshMaterial3d`, `MeshMaterial2d`). An entity is re-keyed when any
  of those components change.
- `vendor/bevy_render/PATCH.md` and `docs/build.md` updated.

## Capabilities

### Modified Capabilities

- `headless-run`: ties in unbatchable and non-mesh bins, and ties between entities that differ
  only in rotation or material, draw in a stable order.
