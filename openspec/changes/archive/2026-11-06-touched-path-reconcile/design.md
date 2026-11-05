## Context

`edit.rs` `write_inner` today: read the file, check the base hash, `writer::apply` (splice, two parses, verify), `check_world`, write atomically, `rekey` renamed paths, `load_text` (parse again, replace the asset), `reconcile_affected`. The last is `reconcile_level` per open level the file is in: `compose_level` (prepare the whole level and its includes, expand every instance in one host call, finish), then `reconcile_external` (resolve every node, `apply_root` with `Prune::Root`), then `stamp_instances` and `stamp_missing` over the root, then `LevelRoot` fields replaced whole.

What stop-incremental left that this builds on:
- `apply_at(world, root, items, AtMode::Reconcile)` (`bevy-ts-tsx/src/reconcile.rs:269`): per-path diff, `None` despawns, retained tree, kept tree and watch journal updated.
- The kept resolved tree per external root (`RootState.resolved`).
- `stamp_instances` and `stamp_missing` take `only`, the key paths to stamp.
- `restore::has_callbacks` answers per root.

Measured split of one write at 20,000 entities (scene-expansion-cache research, branch `scene-expansion-cache`), ms:

| step | today | compose-regression fixed (assumed) | this plan |
|---|---:|---:|---:|
| writer (splice, 2 parses, verify) | 36-48 | 36-48 | 36-48 |
| asset parse | 11-15 | 11-15 | 0 |
| compose without expansion | 165-276 | about 44 | under 2 |
| expansion | 52-65 | 52-65 | under 2 |
| tsx resolve and diff | 83-86 | 83-86 | under 2 |
| stamp and other | about 10 | about 10 | under 5 |
| **write** | **358-447** | **about 240-270** | **under 50** |

At 2,000 entities today's write is about 26 ms (levels-scale); the plan's target is under 8.

## Goals / Non-Goals

**Goals:**
- An edit touching a few entities reconciles only those, at any level size.
- The world after a partial reconcile is the one the full path makes.
- Under 50 ms per write at 20,000 entities, under 8 at 2,000.

**Non-Goals:**
- External changes (a file another program saved). The watcher's reload still reconciles the whole level. Diffing parsed files to find touched paths is a later stage.
- Sidecar levels (`set_overlay`): they take the full path.
- The writer's own cost (splice, verify).
- The expansion cache. A partial reconcile expands only the units' instances, so it needs none.

## Decisions

1. **Units come from the writer.** `Written` gains `units: Vec<(Vec<String>, bool)>`: each name path the batch changed, in the file's name space, and whether it was removed. Per op:
   - `set`, `revert`, `set_prop`, `revert_prop`, `add_component`, `remove_component`: the entity's path.
   - `add`, `restore`: the created path.
   - `delete`: the path, removed.
   - `reparent`: the old path, removed, and the new one.
   - `rename`, and an unnamed entity the writer named with the positional names after it: each old path removed, each new path. `rekey` has moved the entities already; the unit reconciles them in place.
   - an add, restore, delete or rename of an unnamed entity (`#n`) shifts the positional names of the siblings after it without a rename: its parent is the unit (the top level: the full path).
   Units are mapped through the batch's later renames and positional shifts, as `created` is, so they name paths in the new text. Units are reduced to the highest: none under another. Alternative: diff the old and new parse. Rejected for writes: the writer already knows, and the diff is the external-change stage's.

2. **Units map into each root.** For each open level the file is in, `placements` already maps a file name path to the root's key path (`Placement::key_of`). A unit that does not map sends that root down the full path.

3. **The unit rises to the instance.** Walking a unit's key path from the top of the root, the first ancestor whose base is a scene component instance becomes the unit: the instance's children are its expansion merged with the file's overrides, so they compose together. An include ancestor does not stop the walk: an include is read, not expanded.

4. **`compose::prepare_at(path, files, schema, tolerant, units)`.** The same walk as `prepare`, with a key-path trie in `Env`, walked by key path, not by each file's name path: an include of one top entity merges it at the including entity's key, an include of several puts them under it. A scene component instance met above a unit (decision 3) restarts the walk with the instance as the unit. `build_list` builds an entry only if its key is on a unit's path; at or under a unit it builds everything. Ancestors are built with their patches and frames, their other children skipped. Only the units' instances become `ExpandRequest`s. `Prepared::finish` gives `Composed` with `nodes` as `(key path, Option<NodeDesc>)` per unit (`None`: not found, so removed), and `owners`, `scenes`, `missing`, warnings and asset paths for the units only. `prepare` and `prepare_at` share `build_list`; the whole-level compose is `prepare_at` with no filter. One owner, so the two cannot drift.

5. **Per-path state in `LevelRoot`.** `owners`, `scenes` and `missing` are already by key path: a unit replaces the entries at and under it. `warnings` and `assets` become by key path too (the node they arose under; file-level warnings under the empty path, which only the full path replaces). `applied` takes the written file's new hash. `files` stays: a batch that adds or removes an include takes the full path.

6. **`reconcile_at(world, level, units)` in `engine.rs`.** Compose at the units, resolve each unit's subtree with its parent's key path and types as prefix (`resolve_tree`), `apply_at(root, items, Reconcile)`, `stamp_instances` and `stamp_missing` with `only` = the units, patch `LevelRoot`, emit `loaded`. Any error in the compose or resolve falls back to `reconcile_level`, which reports it as today.

7. **The full path, and when.** `write_inner` calls `reconcile_affected` (today's) when:
   - the batch adds or removes an include (`includes_of` before and after differ), or the file's single top changes (`single_top`);
   - the root is a sidecar, dirty, has an `error`, or has no kept resolved tree;
   - a unit's kept subtree or new description holds a binding, handler, ref or slot (tsx's subtree callback check). Their ids belong to the expansion call that made them; re-expanding one instance alone is the expansion cache stage's question;
   - a unit does not map into a placement;
   - the write was rebased: the disk's change reconciles whole first, as today, and the write's own units then reconcile partially.
   A `PartialEdits(bool)` resource, on by default, turns partial reconciles off: the bench's "today" arm and the equivalence test use it.

8. **tsx: placement and rekey.** `apply_at` with `Reconcile` already orders a new node's entity among its siblings (`order_children`). The kept tree's `replace_node` puts a new node last: it takes the node's index from the unit's description instead. `rekey` moves the kept tree's node to its new path with the entity, so a renamed unit reconciles against its own description. A top-level node has no parent entity; its order is `RootState.order`, which `record_order` already sets.

9. **The asset takes the writer's parse.** `Written` gains `parsed: Arc<ParsedFile>`, the verify step's parse of the new text. `load_text` takes it instead of parsing again.

10. **`EditSteps`.** A resource levels writes on each `write_level`: ms per step (writer, parse, compose, expand, reconcile, stamp, total), whether the write took the full path and why, instances expanded, nodes checked. Not in the protocol. The bench reads it; tests read the counts, never the times.

## Risks / Trade-offs

- [A partial compose misses something outside the unit that affects it] → what crosses entities today is the ancestor chain (frames, include merges, instance bases), which the walk builds. The equivalence test applies every op kind partially and fully on the same files and compares `tsx_tree`, components and `LevelInstance`s. A miss leaves the world stale until the next full reconcile.
- [Warnings and assets by key path change their order in `loaded`] → `loaded`'s diagnostics come from them. Order by key path, then as composed; the protocol does not promise order.
- [A unit high in the tree (a top entity holding the whole level)] → its reconcile costs a whole-level one, no worse than today.
- [compose-regression restructures compose] → `prepare_at` sits on its structure; the filter is a trie consulted where entries are built.
- [Positional renames make many units] → each is a rekeyed node reconciled in place; cost scales with them, not the level.

## Migration Plan

compose-regression did not land (checked 2026-11-05): the full path keeps today's compose, 165 to 276 ms at 20,000 entities; the partial path does not depend on it. No protocol or file format change. Rollback: revert, or `PartialEdits(false)`.

## Open Questions

- Can one instance with callbacks re-expand alone? Needs how JS frees a call's handler and binding ids. Until then such a root takes the full path.
- External changes: diff the old and new parse into units? Same `reconcile_at`, a later stage.
- Sidecar levels: `set_overlay` at key paths.
