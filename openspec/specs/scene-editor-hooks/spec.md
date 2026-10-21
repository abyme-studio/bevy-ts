# scene-editor-hooks Specification

## Purpose
A scene editor needs to map what it selects back to source. Every reconciled entity carries its key path and source chain, each prop its provenance and last applied value, and the protocol's `bevy-ts/tsx_*` methods expose them, so an edit written to source round-trips through reload.

## Requirements

### Requirement: Every reconciled entity maps to its TSX element
Each `TsxNode` SHALL expose its node id, scene root, key path, the type of each element along it, and its source chain: the location (project-relative file, line, column) of the element that made the entity, then each prefab call site above it, innermost first. The locations SHALL be those of the generation currently applied.

#### Scenario: Selecting an enemy in the viewport
- **WHEN** a client resolves enemy `b`'s entity through the protocol
- **THEN** it gets `enemy.tsx` with the `<entity>` line inside `Enemy`, then `game.tsx` with the `<Enemy key="b">` line

### Requirement: Prop provenance and last applied values
For each component element on a node, the reconciler SHALL expose each field's provenance (`controlled`, `initial`, `bound`, `default`), the last value tsx applied, and the source location of the element that set it. It SHALL NOT claim whether a prop is editable in source; that is the editor's call after parsing the location.

#### Scenario: Reading enemy b's Health
- **WHEN** a client asks for enemy `b`'s props
- **THEN** `Health.hp` is `initial` with 250, `Health.max` is `controlled` with 250, and both point at `enemy.tsx`'s `<Health>` line

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

### Requirement: An edit written to source round-trips through reload
A controlled prop edited in source by any tool SHALL reach the world through the normal reload path, and the `bevy-ts/tsx_tree+watch` message for that reconcile SHALL name the nodes whose values were written. An initial prop edited in source SHALL NOT change running entities; a client that wants the running value changed too SHALL also call `bevy-ts/mutate_components`.

#### Scenario: An editor drags a cube's position
- **WHEN** a tool rewrites `<Enemy key="b" at={[3, 0, -4]} />` to `at={[5, 0, -4]}` in `game.tsx` and saves
- **THEN** after the reload enemy `b`'s `translation` is `[5, 0, -4]` and the watch stream reports `b` as written
