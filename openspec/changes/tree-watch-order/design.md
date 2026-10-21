## Context

`RootState.order` holds each node's (and slot's) index among its described siblings; `tsx_tree` sorts by it (`description_order`). The watch journal's `Change` records added, removed, tombstoned, moved (source location) and written ids. Nothing records that a sibling list changed, and `record_order` overwrites indices without comparing.

## Goals / Non-Goals

**Goals:** a client keeps its tree in order from deltas alone: adds, moves within a file, reparents, removes.

**Non-Goals:** per-row indices or before-ids; changing `moved` (it stays source locations).

## Decisions

- Full child lists, not an index or a before-id. A message merges every reconcile since the last one, and indices go stale across them; a full list is right whatever happened in between.
- The journal records which lists changed, `(root, parent)`, and `message` builds each list from the live tree when it sends. The latest order wins without merging.
- A list's key is the nearest ancestor row, not the key-path parent: slot steps (`<Show>`, `<For>`) are in the key path but are not rows. `parent` on a row is that same ancestor, so `ordered` and `parent` agree by construction. One helper computes it, used by both.
- A list changed when its child ids, in order, differ before and after a reconcile. `reconcile` snapshots the lists it touches, then compares; a reparent touches both old and new parent. Lists whose parent is gone are dropped at send time.
- `removed` expands to the whole subtree: if a despawn forgets only the top, `forget` walks the root's nodes under its key path.
- `root` and `parent` go on `TsxTreeNode`, so snapshot rows, `tsx_tree` and `added` share one row type. Both optional in the schema, for a minor bump.

## Risks / Trade-offs

- A list of a big flat level (hundreds of siblings) is resent whole on each add: ids only, a few KB. Accepted.
