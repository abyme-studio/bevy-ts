## Context

TS systems are declared by their param types (`reader.rs` reads them into the manifest), built
into `QueryParam`s at load (`systems.rs` `build_query`, through `QueryBuilder`, so
`DefaultQueryFilters` apply), and run with one call into JS per run over zero-copy table columns
(`run_one`). `Changed`/`Added` already narrow a table with a row list (`TableFrame.rows`).
Commands are a binary arena applied after the run (`commands.rs`), with entity ids from a
256-entry pool. Lifecycle observers are queued and run at the next stage boundary
(`observers.rs`). Research with traces and the probe: `docs/research/ecs-queries.md`.

Two bugs: the run walks whole tables, which is wrong when a query names a sparse-set component
(sparse-set components do not split tables), and `Changed`/`Added` on one pass every row. And a
deferred `Remove` observer cannot read the value it was told about.

## Goals / Non-Goals

**Goals:** every row of `ecs-coverage` under Bevy's name, each with a test; the two bugs fixed;
the dense query path unchanged.

**Non-Goals:** `entity(e).observe` and observers on Rust events (rust-events); one-shot systems,
`anyWithComponent` and state-scoped entities (ecs-schedules); inserting reflect-tier components
such as `Disabled` from commands (reflect-coverage stage 2) and `Children` as query data (its stage
3); `SpawnDetails`, `Allow`, `sort`, `entry`, `queue` (low value in TS); copying sparse-set columns
in and out so they can be query data.

## Decisions

1. Sparse sets in two steps. Step one, a plain fix: `build_query` refuses any sparse-set component
   (from `world.components().get_info(id).storage_type()`) with a clear error, so no query returns
   wrong rows. Step two, ask-first (performance): `QueryParam` keeps `is_dense` (Bevy's rule:
   every data, filter and disabling component is `Table`); a non-dense query builds, per matched
   table, a row list from its matched archetypes' `table_row`s and passes it as `rows`, the path
   `Changed`/`Added` use, so JS is unchanged; `Changed`/`Added` on a sparse-set component read
   `sparse_sets.get(id)` ticks by entity. Sparse-set data stays refused: a zero-copy column cannot
   point at a sparse set, whose dense array is not in table order. Dense queries take today's code.
   Alternative: always iterate archetypes. Costs every query a row list; rejected.
2. Filters. `Or`'s archetypal part goes to `QueryBuilder::or`; its row terms join the rows pass
   (a row passes if any term holds; a `With`/`Without` term is constant per table). `Spawned`
   reads `entity_get_spawn_or_despawn_tick` in the rows pass. `Has<C>` is one bit per table
   (`table.has_column`), sent with the frame, so JS reads a constant per table. `AnyOf` is an `or`
   of `with`s plus optional columns. `Ref<C>` hands JS the changed and added tick slices, as `Mut`
   hands changed ticks, and gets a read-only accessor with the tick methods.
3. `Single`/`Populated` count in Rust after the rows pass and skip the call, on the skip path
   run conditions use (`systems.rs`, the condition check before the JS call). ecs-schedules adds
   conditions on the same path; whichever lands second keeps both. Alternative, throw in JS:
   that is today's `single()`, and makes an error row where Bevy skips.
4. `RemovedComponents<C>` reads `world.removed_components()` with a per-identity cursor in
   `SystemMemory`, as TS `MessageReader` cursors are kept, so it survives a reload.
5. Query methods (`iterMany`, `getMany`, `contains`, `isEmpty`, `iterCombinations`,
   `iterAncestors`) are plain JS over the existing `get` index and rows. `iterDescendants` needs
   `Children` as query data, which reflect-coverage stage 3 brings; its task waits on that.
6. Lifecycle values. `Remove`, `Discard` and `Despawn` observers copy the layout-tier bytes of `C`
   into `Pending.bytes` at trigger time (the slot exists, empty today); `On` reads them through the
   component's accessor, as it reads an event's fields. `Discard` and `Despawn` are two more arms in
   `observers::install`, `Lifecycle` and the reader. Bevy fires `Despawn`, `Discard`, `Remove` in
   that order for a despawn; the queue keeps trigger order. Alternative, document that the value is
   gone: makes `Discard` useless.
7. No component hooks. A hook runs inside the structural change, with the world borrowed; JS
   cannot run there. Immediate observers would need the same. Observers with the copied value
   cover what a TS hook would do.
8. Spawn pool. When the pool runs out JS calls a host function `reserve(n)` that allocates `n`
   more ids with `entity_allocator().alloc()` and grows the pool; the run goes on. One host call
   per 256 spawns past the first 256; none in the common case. Alternative: allocate lazily in Rust
   at apply and patch ids. Breaks "spawn returns a real entity at once".
9. `spawn(<el/>)` returns the element's root. JS takes an id from the pool and sends it as the
   mount op's `entity` field; `tsx/mount` spawns the root at that id with `spawn_empty_at` (the
   call `commands.rs` uses) instead of a fresh one, and applies components passed beside the
   element to that root. `entity(e).insert(<el/>)` sends `e` the same way, so the root's
   components land on `e`, which must exist. A fragment has no single root and is refused at the
   call. Why the root, not a parent: Bevy's `spawn_scene` returns the root, and one entity is what
   `spawnNative("cuboid")` made, so migrated code keeps working.
   Agreement with reflect-coverage (lead@reflect-enums-color, 2026-10-23): its `cuboid` migration
   row (`reflect-coverage/design.md:115`) drops "the mesh is a child". Under this rule
   `commands.spawn(Transform.of(...), <entity><mesh/><material/></entity>)` is one entity with
   `Transform`, `Mesh3d` and the material. If reflect-coverage lands first, the old child rule holds
   until this change lands; callers looking for the mesh on a child move with this change (none
   found in tests).
10. `EntityCommands` methods are new ops, or flags on `OP_INSERT`/`OP_REMOVE`, each applied with
   Bevy's method of the same name (`insert_if_new`, `try_insert`, `retain`, `clear`,
   `remove_with_requires`, `clone_and_spawn`, `insert_recursive::<Children>`,
   `remove_recursive::<Children>`, `add_children`, `detach_all_children`, `despawn_children`).
   `withChildren` encodes spawns with `setParent`; `spawnBatch` and `insertBatch` are one op each.
11. Required components (ask-first, new pattern). Bevy 0.20 has no dynamic API:
   `register_required_components` is generic over Rust types. bevy-ts registers an `on_add` hook
   by id (`register_component_hooks_by_id`) on each TS component with `static require`, which
   inserts the missing ones, by id and default bytes, through `DeferredWorld` commands. Every
   insert path meets it. They land one flush later than Bevy's same-bundle insert, still before
   the next system. Alternative: add them in JS at encode time. Cheap, but TSX, levels and Rust
   inserts miss them.
12. Custom relationships (ask-first, new pattern). Bevy's relationship hooks live in the typed
   `Relationship` impl, and a target is a `Vec<Entity>`, which a TS layout (plain bytes, no drop)
   cannot hold. bevy-ts would register relationship hooks by id on the source, keep each target's
   list in a Rust-owned component (registered with a drop and a clone), read as query data through
   a host call per row, and despawn sources for `linkedSpawn`. Alternative: decline, and use
   `Children` plus a TS `Entity` field. Lands nothing that touches the layout tier's rule.
13. Spec deltas. rust-events modifies the `Observers` requirement, so this change adds
   `Discard`/`Despawn` as a new requirement rather than editing that one. reflect-coverage removes
   `Native spawn recipes`; this change's `Structural changes` still names `spawnNative`, and
   whichever archives second drops it.

## Risks / Trade-offs

- [The sparse-set path costs a pass over the archetypes' entities per run] → only non-dense
  queries pay it, which is what Bevy pays; a bench line before and after (`bob_bench` dense,
  plus a sparse variant) lands with it.
- [Required components arrive one flush late] → before the next system, which is what a TS reader
  can observe; documented.
- [The value copy adds bytes per `Remove`/`Discard`/`Despawn` event] → only for watched
  components, plain bytes, no allocation beyond the `Vec` the queue holds.
- [`spawn(<el/>)` changes meaning for existing callers] → no caller in the repo or its tests
  spawns an element with components; the change is listed as breaking in the proposal and in
  `docs/ecs.md`.
- [Overlap with ecs-schedules on the skip path] → named in decision 3; the second to land rebases.

## Migration Plan

Stages 1 to 6 land without the operator's go. Stage 7 (sparse-set filters, required components,
custom relationships) waits for it and may land later or not at all; until it lands, a sparse-set
filter is refused at load (stage 1), and `static require` and `Relationship` fail the load naming
the missing support. If the operator declines an item, its requirement is removed from the delta
before archive and its `ecs-coverage` row marked declined with the reason.

## Open Questions

None blocking. The three ask-first items wait on the operator.
