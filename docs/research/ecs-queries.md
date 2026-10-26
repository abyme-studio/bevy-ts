---
title: ecs-queries research
date: 2026-10-23
bevy: 0.20.0 (Cargo.lock)
source: docs/research/bevy-coverage.md, ECS rows (branch bevy-coverage-audit, 314f032)
---

# ecs-queries research

What Bevy 0.20 gives a system for queries, commands, lifecycle and hierarchy; what a TS system has
today; the gap; and the TS shape that keeps Bevy's names. Bevy paths are under
`bevy_ecs-0.20.0/src`. TS paths are this repo at `70880c6`.

## Headlines

- A. Disabled: yes, TS queries skip `Disabled` entities, and `With<Disabled>` opts in, as in Bevy.
  `build_query` builds through `QueryBuilder::build`, which applies `DefaultQueryFilters`. Proved by
  a probe test (below).
- B. Sparse sets: no. `Changed<C>`/`Added<C>` on a sparse-set component pass every row of every
  matched table, every run. Worse, any sparse-set component in a query (`With`, `Without`, data)
  returns wrong rows, because the run iterates whole tables, and a sparse-set component does not
  split tables. Only Rust components can be sparse-set (TS components are always `Table`,
  `components.rs:158`), so today this bites only Rust markers. Proved by the probe.
- Gap: 13 of the 13 asked rows are missing or partial. Two are a new architectural pattern
  (custom relationships; required components on TS components, which Bevy 0.20 has no dynamic API
  for). Two more items the audit missed are bugs, not features: B, and lifecycle `Remove`
  observers run after the component is gone.

## A. Do TS queries skip `Disabled`?

Yes.

Trace:

1. `systems.rs:393` builds every TS query with `QueryBuilder::<Entity>::new(world)` and
   `b.build()` (`systems.rs:421`).
2. `QueryBuilder::build` calls `QueryState::from_builder` (`query/builder.rs:271`), which reads the
   world's `DefaultQueryFilters` and calls `modify_access` (`query/state.rs:323-325`).
3. `modify_access` adds `without(Disabled)` unless the query names `Disabled` itself
   (`entity_disabling.rs:226-232`). `World::bootstrap` registers `Disabled` as a disabling
   component (`entity_disabling.rs:135`, `:185`).
4. `Disabled` is table-stored (`entity_disabling.rs:128-136`, no `storage` attribute), so a
   disabled entity sits in its own table, which the query does not match. The run walks
   `matched_tables()` only (`systems.rs:686`).

So `Query<[Plain]>` skips disabled entities and `Query<[Plain], With<Disabled>>` sees only them,
as Bevy does. Two caveats:

- The filters are read once, when the generation loads. A disabling component a Rust plugin
  registers after that is not seen until the next reload. Bevy has the same rule for a
  `QueryState`.
- TS has no `Allow<T>` (Bevy's opt-in to disabled entities without filtering on them). `With<T>`
  and `Option<T>` both name `T`, so `Option<Disabled>` works as "all, disabled or not".

Proof: `probe_disabled_and_sparse_filters` (below).

## B. Do `Changed` and `Added` work on sparse-set components?

No. And sparse-set components break `With` and `Without` too.

Trace of `run_one` (`systems.rs:682-745`):

1. It walks `q.state.matched_tables()` (`:686`) and hands JS every entity of each table
   (`:722-726`). That is right only when the query is dense. Bevy itself switches to archetype
   iteration when any component in the query is sparse-set (`query/state.rs:278`, `:654-667`,
   `query/builder.rs:74-96`). A sparse-set component does not move an entity to another table, so
   tagged and untagged entities share one table. `With<Tag>` matches the archetype with `Tag`, its
   table is matched, and every entity in that table comes back, tagged or not. `Without<Tag>`
   comes back the same.
2. `Changed`/`Added` read `table.get_changed_ticks_slice_for(id)` (`:701-702`). A sparse-set
   component has no table column, so this is `None`, `filter_map` drops it, the `all()` over no
   tick slices is `true` (`:705`), and every row passes.
3. Sparse-set data (`Query<[Spark]>`): `table.get_column(id)` is `None` (`:729`), so the column
   frame is null and every row access throws in JS. The load does not refuse it.

Probe results (Bevy's own query in the same world, for comparison). World: four `Plain` entities
(table-stored, layout tier); one also has `Disabled`, one has `Tag`
(`#[component(storage = "SparseSet")]`). A TS system in `PostUpdate` logs each query's `len` for
three runs after load. Same output every run:

| Query | TS `len` | Bevy |
|---|---|---|
| `Query<[Entity, Plain]>` | 3 | 3 |
| `Query<[Entity, Plain], With<Disabled>>` | 1 | 1 |
| `Query<[Entity, Plain], With<Tag>>` | 3 | 1 |
| `Query<[Entity, Plain], Without<Tag>>` | 3 | 2 |
| `Query<[Entity, Plain], Changed<Tag>>`, after the first run | 3 | 0 |

`Query<[Spark]>` on a sparse-set layout component loads with no error, then every row access
throws `TypeError: Cannot read properties of undefined (reading 'v')`.

The probe was a throwaway test in `crates/bevy-ts-cli/tests/` built on `common::{project, app,
reload, messages}`. The fix should add it back as its regression test.

Who it hits: only Rust components, since TS components are always `Table`. In the engine:
`SyncToRenderWorld` (`bevy_extract-0.20.0/src/sync_world.rs:122`), two gizmo markers
(`bevy_gizmos-0.20.0/src/transform_gizmo.rs:87`, `:97`), and any game or port crate's
`#[component(storage = "SparseSet")]` marker, which is Bevy's advice for markers added and
removed often.

What a fix touches (one file, `crates/bevy-ts/src/systems.rs`, plus tests):

- `QueryParam` keeps `is_dense` (Bevy's rule: every data and filter component, and every
  disabling component, is `Table`). `QueryState` keeps it private; compute it in `build_query`
  from `world.components().get_info(id).storage_type()`.
- Not dense: per matched table, build a row list from the matched archetypes in that table
  (`archetype.entities()` gives each `table_row`), and pass it as the frame's `rows`, the same
  path `Changed`/`Added` already use. JS needs no change: `rows` already narrows a table
  (`TableFrame.rows`).
- `Changed`/`Added` on a sparse-set component: read its ticks per entity from
  `world.storages().sparse_sets.get(id)` (`get_changed_tick`/`get_added_tick` by entity, `storage/sparse_set.rs:313`, `:325`) instead of
  the table slice.
- Sparse-set data: refuse at load with a clear error ("`Spark` is a sparse-set component; it can
  be a filter but not query data"). A zero-copy column cannot point at a sparse set: its dense
  array is not in table order. Copying it in and out is the only other way, and nobody asks for
  it.
- Dense queries keep today's path untouched, so the common case costs nothing. A non-dense query
  pays one pass over its archetypes' entities per run, which is what Bevy pays.

This is a bug fix in the existing query path. It is not on the ask-first list, but it does change
the per-frame query path, so it should land with a bench line (dense: unchanged; sparse: rows
built per run).

## 1. Coverage

Bevy column: the name and what it does in 0.20. TS column: what exists (`core.js` is
`packages/bevy-ts/runtime/core.js`, `index.d.ts` is `packages/bevy-ts/index.d.ts`). Shape: the TS
form that keeps Bevy's name.

### Filters and query data

| Item | Bevy 0.20 | TS today | Gap | TS shape |
|---|---|---|---|---|
| `Or` | `Or<(F1, F2)>` filter, any of (`query/filter.rs`). Archetypal for `With`/`Without`, per-row for `Changed`/`Added` | none. `reader.rs:1140` takes only `With`/`Without`/`Changed`/`Added` | missing | `Query<[Enemy], Or<[Changed<Health>, Added<Health>]>>`. Rust: `QueryBuilder::or(|b| …)` for the archetypal part; per-row `Or` of tick tests in the rows pass |
| `Has` | `Has<T>` data: a bool per row, no access to `T`, never filters (`query/fetch.rs`) | none. `reader.rs:1131` refuses any wrapper but `Mut`/`Option` | missing | `Query<[Entity, Has<Stunned>]>` gives `boolean`. Rust: per table, `table.has_column(id)` (dense) or archetype `contains` (sparse); a constant per table, so JS gets one bit per table |
| `Spawned` | `Spawned` filter: entity spawned since the system last ran; reads the entity's spawn tick, not a component (`query/filter.rs:1210`) | none | missing | `Query<[Entity, Enemy], Spawned>`. Rust: `world.entities().entity_get_spawn_or_despawn_tick(e)` (`entity/mod.rs:1036`) against last run, into the same `rows` list |
| `AnyOf` | `AnyOf<(A, B)>`: each as `Option`, at least one present | none | missing (audit listed) | `Query<[AnyOf<[A, B]>]>` gives `[A \| undefined, B \| undefined]`. Rust: `or` of `with`s plus optional columns |
| `Ref` | `Ref<T>`: read plus `is_added()`/`is_changed()`/`last_changed()` | none | missing (audit listed) | `Query<[Ref<Health>]>`: the read view gains `isAdded()`/`isChanged()`, backed by the tick slices JS already holds for `Mut` (hand them over for `Ref` columns too) |
| `Allow` | `Allow<T>`: include entities with disabling component `T` | none; `Option<Disabled>` does the job | missing, low value | `Allow<Disabled>`; Rust: `optional(ref_id)` so the default filter is lifted without adding access |
| `SpawnDetails` | data: spawn tick and caller (`query/fetch.rs`) | none | missing, low value | skip unless asked |

### System params and query methods

| Item | Bevy 0.20 | TS today | Gap | TS shape |
|---|---|---|---|---|
| `Single` | `Single<D, F>`: exactly one match, else the system is skipped (validation, not a panic) (`system/query.rs`) | `q.single()` throws when not one (`core.js:1522`) | partial: a throw is an error row, Bevy skips | param `Single<[Mut<Transform>], With<Player>>`; Rust checks the count before calling JS (as conditions do, `systems.rs:648`) and skips; JS gets the row itself. `Option<Single<…>>` for "zero or one" |
| `Populated` | `Populated<D, F>`: at least one match, else skipped | none | missing | param `Populated<[Enemy]>`; same Rust check, `len > 0`; JS sees a `Query` |
| `iterMany` | `Query::iter_many(entities)`: rows for a list of entities, in list order, skipping non-matches | `get(e)` one at a time (`core.js:1531`) | missing | `q.iterMany(list)` iterable and `q.getMany(list)` (throws on a miss, as Bevy's `get_many`). Pure JS over the existing `get` index |
| `contains`, `isEmpty` | `Query::contains(e)`, `is_empty()` | `get(e) !== undefined`, `len === 0` | cosmetic | add as one-liners for Bevy's names |
| `iterCombinations` | `iter_combinations::<K>()` | none | missing, audit missed | `q.iterCombinations(2)`; pure JS |
| `sort` | `q.iter().sort::<&C>()` and friends (`query/iter.rs`) | none | missing, audit missed; JS `Array.sort` covers it | skip |
| `RemovedComponents` | `RemovedComponents<T>`: entities that lost `T` (removed or despawned) since last read; a message cursor (`lifecycle.rs`) | none. `Remove<C>` observers exist, deferred | missing | param `RemovedComponents<Health>`, iterable of `Entity`. Rust: read `world.removed_components()` messages with a per-system cursor in `SystemMemory`, as `MessageReader` does (`systems.rs:782`) |
| `SystemChangeTick` | this run's tick and last-run tick | `q.tick` only (`index.d.ts:268`) | partial | param `SystemChangeTick` with `thisRun`, `lastRun` |

### Lifecycle and observers

| Item | Bevy 0.20 | TS today | Gap | TS shape |
|---|---|---|---|---|
| `Discard` | `On<Discard, C>`: before a value of `C` is replaced or removed, value still readable (0.20's name for 0.16's `Replace`) (`lifecycle.rs:402`) | `Lifecycle` is `Add`/`Insert`/`Remove` only (`manifest.rs:164`, `observers.rs:56`) | missing | `On<Discard<Health>>`. One more arm in `observers::install` and the reader |
| `Despawn` | `On<Despawn, C>`: entity with `C` is despawned, before removal (`lifecycle.rs:455`) | none | missing | `On<Despawn<Health>>`, same place |
| Lifecycle timing | `Remove`/`Discard`/`Despawn` run while the component is still there; `Add`/`Insert` right after | all lifecycle observers are queued and run at the next stage boundary (`observers.rs:55-63`) | partial, audit missed: in a TS `Remove`/`Discard`/`Despawn` observer the component (or the entity) is already gone, so the observer cannot read the value, which is the point of `Discard` | either copy the component's bytes into the queued event at trigger time (cheap, layout tier is plain bytes), or document it. Recommend the copy: the payload slot exists (`Pending.bytes`) |
| Bubbling | `EntityEvent` with `#[entity_event(propagate)]` walks `ChildOf` | `observers.rs` computes the `ChildOf` path; untested | test only | a test |
| Hooks | `on_add`/`on_insert`/`on_discard`/`on_remove`/`on_despawn`, immediate | none | missing | recommend not: hooks run inside the structural change, where JS cannot run. Observers plus the copy above cover the TS need |

### Commands

| Item | Bevy 0.20 | TS today | Gap | TS shape |
|---|---|---|---|---|
| `spawn(<el/>)` | `commands.spawn(bundle)` returns `EntityCommands`; `World::spawn_scene(scene)` returns the root (`bevy_scene-0.20.0/src/spawn.rs:182`) | `spawn(<el/>)` alone mounts and returns `undefined`; with components it spawns one entity and mounts the element as its child (`core.js:1891-1899`) | partial | return the element's root entity: take an id from the pool (`core.js:1886`) and send it in the mount op; `tsx/mount` spawns the root at that id with `spawn_empty_at` (`bevy-ts-tsx/src/mount.rs:37`, same call `commands.rs:248` uses). Mixed `spawn(Health.of(), <el/>)`: the components go on the element's root, not on a parent |
| `entity(e).insert(<el/>)` | `insert` puts components on `e` | mounts the element as a child of `e` (`core.js:1801-1804`) | partial | mount with `e` as the root node's entity (same `entity` field). Reject an element that is not one entity (a fragment) |
| `spawnBatch` | `commands.spawn_batch(iter)`; ids not returned (`system/commands/mod.rs`) | none; `spawn` in a loop | missing | `commands.spawnBatch(list)`: one op, one id block. Returning ids is fine in TS (Bevy's `World::spawn_batch` does) |
| 256 spawns per run | none | `POOL_SIZE = 256` (`commands.rs:42`), throws past it (`core.js:1887`) | partial | lift: refill the pool from JS on demand with a host call, or allocate more ids in Rust on demand (`entity_allocator().alloc()`, as `commands.rs:417` does). Recommend a host call `reserve(n)` that grows the pool |
| `insertIfNew`, `tryInsert`, `retain`, `clear`, `removeWithRequires` | `EntityCommands` methods | none | missing | same names on `EntityCommands`; each is one op flag or a new op |
| `insertBatch` | `commands.insert_batch(iter)` | none | missing, audit missed | `commands.insertBatch([[e, ...parts]])` |
| `cloneAndSpawn`, `cloneWith` | `EntityCommands::clone_and_spawn` | none | missing | `entity(e).cloneAndSpawn()` returns the new id from the pool |
| `despawn` recursion | `despawn` takes linked children (`ChildOf` is `linked_spawn`) | `despawn` op calls `world.despawn` (`commands.rs:267`), so children go; untested | test only | a test |
| `insertRecursive`, `removeRecursive` | on a relationship target, down the tree (`relationship/related_methods.rs`) | none | missing, audit missed. This is how Bevy disables a tree | `entity(e).insertRecursive(Disabled)` once reflect-coverage stage 2 lets `Disabled` be inserted |

### Hierarchy and relationships

| Item | Bevy 0.20 | TS today | Gap | TS shape |
|---|---|---|---|---|
| `ChildOf`, `setParent` | `ChildOf(parent)` relationship; `Children` target kept by hooks | `setParent` op (`commands.rs:279-287`); `ChildOf` readable | idiomatic | — |
| `Children` read | `Query<&Children>`, `iter_descendants`, `iter_ancestors` (`relationship/relationship_query.rs`) | `Children` is reflect tier, not query data | missing | reflect-coverage stage 3 gives `Query<[Children]>`; this change adds `iterDescendants(e)`/`iterAncestors(e)` on a query that has `Children`/`ChildOf`, as Bevy's `Query<&Children>` methods |
| `with_children`, `add_children`, `detach_children`, `despawn_children`, `children!` | `EntityCommands` and the `children![]` spawn macro | `setParent` only | missing | `entity(e).withChildren((parent) => …)`, `addChildren([…])`, `detachChildren()`, `despawnChildren()`. TSX already nests, so `children!` is not needed |
| Custom relationships | `#[derive(Component)] #[relationship(relationship_target = …)]`, kept by generic hooks; `with_related`, `despawn_related`, `related!` | none | missing; new pattern (below) | `class Likes extends Relationship(LikedBy) { declare target: Entity }`, `class LikedBy extends RelationshipTarget(Likes) {}`; `Query<[LikedBy]>` gives the list |
| Required components | `#[require(B, C = …)]` on a component: inserting it inserts the missing required ones | none on TS components | missing; new pattern (below) | `class Enemy extends Component { static require = [Health, Transform.of({ … })] }` |

### Also missed by the audit

- `Query::get_many` / `iter_many_mut` / `iter_combinations` (above).
- `EntityCommands::queue`, `entry` (`entry::<C>().or_insert(…)`): low value in TS.
- `Commands::run_system` / one-shot systems: owned by ecs-schedules.
- `commands.entity(e).observe` and `trigger` on Rust events: owned by rust-events.
- Lifecycle observer timing (above) and the sparse-set bug (B).

## Two items are a new architectural pattern (ask first)

Required components. Bevy 0.20 has no public way to give a dynamic component requirements:
`World::register_required_components` is generic over Rust types (`world/mod.rs:411`, `:462`),
and the `RequiredComponentsRegistrator` is only built inside typed registration
(`component/register.rs:235`). TS components are registered from a `ComponentDescriptor`
(`components.rs:156`). So bevy-ts must emulate it. Two ways:

1. In JS at encode time: `spawn`/`insert` add the missing required parts. Cheap, but TSX, levels
   and Rust inserts of a TS component miss it.
2. In Rust: a component hook by id (`World::register_component_hooks_by_id`, `world/mod.rs:359`)
   `on_add` that inserts what is missing through `DeferredWorld` commands. Covers every insert
   path; inserts land one command flush later than Bevy (Bevy inserts them in the same bundle).

Recommend 2, done in Rust where every path meets. It is a new pattern (bevy-ts owning a hook on a
TS component).

Custom relationships. Bevy 0.20 describes a dynamic relationship
(`RelationshipAccessorInitializer`, the last argument of `new_with_layout`, `component/info.rs:304`,
`relationship/mod.rs:530`), but the hooks that keep the target's list live in the typed
`Relationship` impl. A target is a `Vec<Entity>`, which a TS layout (plain bytes, no drop) cannot
hold. bevy-ts would own: the relationship hooks by id; a target component that is not plain bytes
(a Rust-owned list, read through a host call or as a reflect-tier value); `linked_spawn` despawn.
That is a new pattern and touches the layout tier's "plain bytes" rule. Recommend landing
`Children` helpers first (Bevy's own relationship, no new pattern) and asking before custom
relationships.

## 4. Neighbours

| Team / change | Overlap | Owner |
|---|---|---|
| reflect-enums-color, change `reflect-coverage` | stage 2 makes `Disabled`, `Name`, `Camera3d` insertable from `Commands`; stage 3 makes `Children` and other reflect-tier components query data. Its migration table maps `spawnNative("cuboid")` to `commands.spawn(Transform.of(…), <entity>…</entity>)` with "the mesh is a child" (`reflect-coverage/design.md:115`), which is today's `spawn(<el/>)` child rule | reflect-coverage: inserting `Disabled`, reading `Children`. ecs-queries: `spawn(<el/>)` semantics, `Children` helpers. Tell reflect-coverage that ecs-queries changes the cuboid recipe's replacement: under the new rule `Transform` goes on the element's root, no child |
| rust-events | `entity(e).observe`, observers on Rust events, bubbling of Rust events. Its non-goals hand `Discard`/`Despawn` and hooks to ecs-queries (`rust-events/design.md:21`) | rust-events: observe from code. ecs-queries: `Discard`/`Despawn`, TS bubbling test |
| bevy-ui-coverage | `NodeRef.has(C)` in TSX, compared by presence, explicitly not `RemovedComponents` (`bevy-ui-coverage/design.md:41`); `disabled` on UI is `InteractionDisabled`, not `Disabled` | bevy-ui-coverage: TSX reads. ecs-queries: `Has<C>` and `RemovedComponents` in systems. No conflict |
| ecs-schedules | `DespawnOnExit`/`DisableOnExit` state-scoped entities; one-shot systems; `anyWithComponent` conditions | ecs-schedules. `Populated`/`Single` skipping overlaps conditions in spirit: both skip in Rust before the JS call; share the skip path |
| script-gltf (`scene-animation`) | walks children in Rust (`bevy_ts_animation::players`, `Query<&Children>`) for players; names "walk children" as a TS gap (`scene-animation/design.md:9`) | script-gltf keeps its Rust walk. ecs-queries' `iterDescendants` is the TS answer |
| rust-resources | `Res` of Rust resources | no overlap |
| mesh-primitives, text-input, world-to-screen, material-alpha, camera-rays | none in queries or commands | — |

No one else owns relationships, `spawn(<el/>)`, `spawnBatch`, required components, `Single`,
`Populated`, `iterMany` or `RemovedComponents`.

## 5. Ask-first

- New architectural pattern: required components on TS components (a bevy-ts hook by id) and
  custom relationships (hooks by id plus a non-plain-bytes target). Ask before either.
- Performance: B's fix changes the per-frame query path for non-dense queries only; dense queries
  keep today's code. Land it with a bench line. `Single`/`Populated` add a count before the JS
  call (Bevy pays the same). Lifting the 256-spawn pool adds a host call only when the pool runs
  out.
- New dependency: none.

## Recommendation

One change, in this order: B (bug fix, with the probe as its test) and the A test; filters and data
(`Or`, `Has`, `Spawned`, `AnyOf`, `Ref`); params (`Single`, `Populated`, `RemovedComponents`,
`SystemChangeTick`) and methods (`iterMany`, `getMany`, `contains`, `isEmpty`);
`Discard`/`Despawn` with the value copied at trigger time; commands (`spawn(<el/>)` returns its
root, `spawnBatch`, the pool, the `EntityCommands` methods, `Children` helpers). Required
components and custom relationships go to the operator first.

## 6. Sparse-set filter bench (stage 7)

`bob_bench`, release, 8 runs per arm, before and after alternating, medians of each run's median
(µs). Before is `ecs-queries-stage7-before` (6dab285b): 7.2's refusal lifted, so `With<SparseTag>`
walks the whole table and gives every row, the wrong answer. After walks each matched
archetype's row list. Every other entity is tagged. The machine was loaded (1-minute load 74 to
115), so spreads are wide.

| entities | variant | before bob run | after bob run | before app.update | after app.update |
|---|---|---|---|---|---|
| 1000 | accessor+tick | 24 [20–35] | 27 [20–48] | 132 | 151 |
| 1000 | With<TableTag> | 26 [22–50] | 28 [22–39] | 154 | 156 |
| 1000 | With<SparseTag> | 29 [21–52] | 30 [24–70] | 184 | 195 |
| 100000 | accessor+tick | 1556 [1399–2194] | 1590 [1276–2063] | 1894 | 1996 |
| 100000 | raw, no tick | 1216 [1096–1662] | 1267 [995–1479] | 1557 | 1713 |
| 100000 | raw+tick | 1174 [1083–1459] | 1154 [1039–1454] | 1492 | 1474 |
| 100000 | With<TableTag> | 882 [735–1038] | 745 [692–806] | 1374 | 970 |
| 100000 | With<SparseTag> | 1621 [1472–2059] | 1891 [1588–2076] | 2182 | 2237 |

Dense queries are unchanged within noise: every dense median moves less than its spread. A
sparse-set filter at 100,000 costs about 17% more than the wrong whole-table walk, though it
hands JS half the rows: the row list is built per frame and JS indexes through it. It is the
same path `Changed` takes, and costs about what a dense query over all rows does.
