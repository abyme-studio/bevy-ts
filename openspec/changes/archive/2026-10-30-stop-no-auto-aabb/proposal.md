## Why

A Stop leaves `Aabb` to Bevy: `calculate_bounds` writes it again from the mesh (`deb20acc6`). It
skips an entity with `NoAutoAabb`, though. World text's labels have one, and so can any entity a
game bounds itself. On such a node a Stop kept the play's bounds: a written-back node kept its
stale `Aabb`, and one whose only change was its `Aabb` was not restored at all. Picking and culling
then read bounds the level does not have.

## What Changes

- `bevy-ts-tsx`'s scan and write-back count `Aabb` as recomputed only on an entity without
  `NoAutoAabb`. On one with it, an `Aabb` changed after the baseline is a play change that tsx did
  not insert, so the node respawns, and the respawn bounds it as a spawn does.
- `docs/world-text.md` says what a Stop does to a label.

## Capabilities

### Modified Capabilities

- `scene-reconciler`: `Aabb` is recomputed only where Bevy computes it.
