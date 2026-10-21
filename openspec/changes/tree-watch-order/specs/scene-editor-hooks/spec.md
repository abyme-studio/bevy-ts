## MODIFIED Requirements

### Requirement: Protocol methods for the scene tree
bevy-ts SHALL add to `protocol/bevy-ts.openrpc.json`, as a minor version bump: `bevy-ts/tsx_tree` (every root with its nodes, in description order with each parent before its children, a level's in file order: node id, entity, key path, element type, source, tombstoned, `root`, and `parent`, the node id of the nearest ancestor row or null at the root's top), `bevy-ts/tsx_node` (`entity` or `node`: the source chain, components and provenance above), `bevy-ts/tsx_find` (`file`, `line`, `column`: every entity whose source chain contains that location, so one prefab element maps to all its instances) and `bevy-ts/tsx_tree+watch` (the tree, then one message per reconcile with nodes added, removed, tombstoned and source locations moved).

A watch message SHALL list in `removed` every node gone, its whole subtree included. It SHALL carry `ordered`, one `{ root, parent, children }` per sibling list that gained a child, lost one or changed order since the last message: `children` is that list's full order, as `tsx_tree` would give it. A list whose parent was removed SHALL NOT be sent. A reconcile that changes only order SHALL send a message. A client that applies `added` and `removed`, then replaces each listed parent's children with `children`, SHALL hold the same tree `tsx_tree` returns.

#### Scenario: Finding all instances of a prefab element
- **WHEN** a client calls `bevy-ts/tsx_find` with the `<entity>` line inside `Enemy`
- **THEN** it gets every enemy entity, including runtime-mounted ones

#### Scenario: A move within a level
- **WHEN** `levels/arena.bsn` swaps its top-level entities `A` and `B` and reloads
- **THEN** the watch message carries `ordered` for the level root's top (`parent` null) with `B` before `A`, and no `added` or `removed`

#### Scenario: An add between siblings
- **WHEN** a `level_write` adds `C` at index 1 under `A`, whose children were `a1`, `a2`
- **THEN** the watch message carries `C` in `added` with `parent` `A`, and `ordered` for `A` with `a1`, `C`, `a2`

#### Scenario: A reparent
- **WHEN** a `level_write` reparents `a2` from `A` to `B`
- **THEN** the watch message carries `ordered` for both `A` and `B`

#### Scenario: Removing a parent
- **WHEN** a `level_write` deletes `A`, which has children `a1` and `C`
- **THEN** `removed` lists `A`, `a1` and `C`, and no `ordered` entry has `parent` `A`
