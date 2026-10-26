Stages 1 to 6 are plain fixes and rows on existing patterns, and land without stage 7. Stage 7
holds the ask-first items; the operator gave both their go on 2026-10-22. Tests named here are in
`crates/bevy-ts-cli/tests/ecs_queries.rs` (new, on `common::{project, app, reload, messages}`)
unless a path is given; each `ecs-coverage` row's test is written with its feature.

## 1. Fixes and tests for what exists

- [x] 1.1 `systems.rs` `build_query`: refuse a sparse-set component as data, filter or `Has`, with "`<name>` is a sparse-set component; it can be a filter but not query data" for data and "sparse-set filters need the sparse-set query path" for filters, file and line; `sparse_set_data_is_refused`
- [x] 1.2 `probe_disabled_and_sparse_filters` (the research probe back as a test): `Disabled` rows as in the research table; the sparse-set filter queries refused at load until stage 7
- [x] 1.3 `despawn_takes_children`, `remove_takes_only_named`, `entity_event_bubbles_until_stopped`

## 2. Filters and query data

- [x] 2.1 `reader.rs` and `manifest.rs`: `Or`, `Spawned` filters; `Has`, `AnyOf`, `Ref` data; their load errors
- [x] 2.2 `systems.rs`: `Or` (builder `or` plus row terms), `Spawned` in the rows pass, `Has` bit per table, `AnyOf`, `Ref` tick slices
- [x] 2.3 `core.js` and `index.d.ts`: the `Has` boolean, `AnyOf` tuple, `Ref` accessor with `isAdded`/`isChanged`/`lastChanged`, writes through `Ref` throwing
- [x] 2.4 Tests: `or_mixes_archetype_and_row_terms`, `has_is_a_bool_and_never_filters`, `spawned_sees_new_entities_once`, `any_of_needs_one_part`, `ref_reports_ticks_and_refuses_writes`

## 3. Params and query methods

- [x] 3.1 `Single`, `Option<Single>`, `Populated`: reader, manifest, a count after the rows pass and a skip on the run-condition skip path; `single_skips_without_exactly_one`, `populated_skips_when_empty` (no JS call recorded)
- [x] 3.2 `RemovedComponents<C>` with a cursor in `SystemMemory`; `removed_components_sees_remove_and_despawn_once`, including a reload between runs
- [x] 3.3 `SystemChangeTick`; `system_change_tick_last_run_is_previous_this_run`
- [x] 3.4 `core.js`: `contains`, `isEmpty`, `iterMany`, `getMany`, `iterCombinations`, `iterAncestors`; `contains_and_is_empty`, `iter_many_and_get_many`, `iter_combinations_yields_each_set_once`
- [x] 3.5 `index.d.ts`: the new params and methods, with JSDoc naming Bevy's equivalent

## 4. Lifecycle

- [x] 4.1 `Lifecycle::Discard` and `Lifecycle::Despawn` in `manifest.rs`, `observers::install` and the reader
- [x] 4.2 Copy `C`'s layout bytes into `Pending.bytes` at trigger time for `Remove`/`Discard`/`Despawn`; `On` reads them through the accessor
- [x] 4.3 `remove_observer_reads_the_value`, `discard_sees_the_old_value`, `despawn_observers_run_in_bevy_order`

## 5. Commands

- [x] 5.1 Pool refill: host call `reserve(n)` in `runtime.rs`, `alloc` in `core.js` calls it when empty; `a_thousand_spawns_in_one_run` (at most 4 refill calls)
- [x] 5.2 `spawn(<el/>)` returns the root: id from the pool in the mount op's `entity`; `bevy-ts-tsx/src/mount.rs` spawns the root there with `spawn_empty_at`; side parts go on the root; fragments and duplicate components throw; `spawn_element_returns_its_root`
- [x] 5.3 `entity(e).insert(<el/>)` mounts with `e` as the root; `insert_element_lands_on_the_entity`
- [x] 5.4 `spawnBatch`, `insertBatch`; `spawn_batch_returns_ids_in_order`, `insert_batch_inserts_each`
- [x] 5.5 `insertIfNew`, `tryInsert`, `retain`, `clear`, `removeWithRequires`, `cloneAndSpawn`, `insertRecursive`, `removeRecursive` in `commands.rs` and `core.js`; `insert_if_new_and_try_insert`, `retain_and_clear`, `remove_with_requires` (on a Rust component with `#[require]`), `clone_and_spawn_copies_components`, `insert_and_remove_recursive`
- [x] 5.6 `withChildren`, `addChildren`, `detachChildren`, `despawnChildren`; `children_commands`
- [x] 5.7 Move any caller that relied on the child rule for `spawn(<el/>)` or `insert(<el/>)` (`crates/bevy-ts-v8/js/test/jsx-runtime.test.js`, examples); tell lead@reflect-enums-color when 5.2 lands

## 6. Hierarchy walk, docs and gate

- [x] 6.1 `iterDescendants` on a query with `Children` data, once reflect-coverage stage 3 has landed on main; `iter_ancestors_and_descendants`. If it has not landed when stages 1 to 5 are done, land them and leave this task for a follow-up commit on main
- [x] 6.2 `docs/ecs.md`: queries (filters, data, `Single`/`Populated`, methods), `RemovedComponents`, lifecycle observers and the copied value, commands (the `spawn(<el/>)` root rule, marked breaking), hierarchy, `Disabled`, sparse-set limits; link it from `docs/scripts.md`
- [x] 6.3 Every `ecs-coverage` row's test exists; rows not landed say so
- [ ] 6.4 Gate once: workspace build, tests and clippy; `pnpm` type tests for `index.d.ts`

## 7. Ask-first (operator's go given 2026-10-22; each item lands on its own)

- [x] 7.1 Bench before: `bob_bench` dense numbers, plus a new sparse variant (a `With<Tag>` sparse-set filter over 1,000 and 100,000 entities) recorded with stage 1's refusal lifted behind a test flag
- [x] 7.2 Sparse-set filters: `is_dense` on `QueryParam`; per-table row lists from matched archetypes for non-dense queries; `Changed`/`Added` on a sparse-set component from per-entity sparse ticks; `Has` per archetype; lift 1.1's filter refusal (data stays refused); `probe_disabled_and_sparse_filters` asserts Bevy's lengths
- [x] 7.3 Bench after: dense unchanged within noise, sparse cost recorded in `docs/research/ecs-queries.md`
- [x] 7.4 Required components: `static require` in the reader and manifest; an `on_add` hook by id inserting missing parts through `DeferredWorld` commands; cycle error; `required_components_from_every_path` (commands, TSX, level, Rust insert)
- [x] 7.5 `Relationship` and `RelationshipTarget` are not exported; the `ecs-coverage` row names custom-relationships as their owner
