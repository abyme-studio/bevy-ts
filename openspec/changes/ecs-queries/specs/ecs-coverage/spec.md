## ADDED Requirements

### Requirement: Every ECS query, command, lifecycle and hierarchy item has a TS form and a test
For each Bevy 0.20 item below, bevy-ts SHALL offer the TS form named, under Bevy's name, and the test named SHALL prove it. Tests are in `crates/bevy-ts-cli/tests/ecs_queries.rs` unless a path is given. An item marked "declined" SHALL have no TS form, for the reason given. An item marked "owned by" another change is that change's to land, with its test. The audit row each item closes is from `docs/research/bevy-coverage.md` (ECS); "research" marks one the audit missed (`docs/research/ecs-queries.md`).

| Item (Bevy 0.20) | TS form | Test | Source |
|---|---|---|---|
| `Or<(F1, F2)>` | `Or<[F1, F2]>` filter | `or_mixes_archetype_and_row_terms` | audit: filters |
| `Has<T>` | `Has<T>` data, `boolean` | `has_is_a_bool_and_never_filters` | audit: filters |
| `Spawned` | `Spawned` filter | `spawned_sees_new_entities_once` | audit: filters |
| `AnyOf<(A, B)>` | `AnyOf<[A, B]>` data | `any_of_needs_one_part` | audit: filters |
| `Ref<T>` | `Ref<T>` data with `isAdded`/`isChanged`/`lastChanged` | `ref_reports_ticks_and_refuses_writes` | audit: filters |
| `Query::iter_many` | `q.iterMany(list)` | `iter_many_and_get_many` | audit: params |
| `Query::get_many` | `q.getMany(list)` | `iter_many_and_get_many` | research |
| `Query::iter_combinations` | `q.iterCombinations(k)` | `iter_combinations_yields_each_set_once` | research |
| `Query::contains`, `is_empty` | `q.contains(e)`, `q.isEmpty()` | `contains_and_is_empty` | research |
| `Single<D, F>` | `Single<[...], F>`, `Option<Single<...>>` param | `single_skips_without_exactly_one` | audit: params |
| `Populated<D, F>` | `Populated<[...], F>` param | `populated_skips_when_empty` | audit: params |
| `RemovedComponents<T>` | `RemovedComponents<T>` param | `removed_components_sees_remove_and_despawn_once` | audit: params |
| `SystemChangeTick` | `SystemChangeTick` param | `system_change_tick_last_run_is_previous_this_run` | audit: params |
| `Children`, `iter_descendants`, `iter_ancestors` | `q.iterDescendants(e)` (with `Children` data), `q.iterAncestors(e)` (with `ChildOf`) | `iter_ancestors_and_descendants` | audit: hierarchy walk |
| `with_children`, `add_children`, `detach_children`, `despawn_children` | `entity(e).withChildren(fn)`, `addChildren`, `detachChildren`, `despawnChildren` | `children_commands` | audit: hierarchy walk |
| Custom relationships, `with_related`, `despawn_related` | owned by custom-relationships, a change of its own after this one | — | audit: relationships |
| `spawn` returning the entity for a scene root | `spawn(<el/>)` returns the root; parts go on it | `spawn_element_returns_its_root` | audit: `spawn(<el/>)` |
| `EntityCommands::insert` of a scene | `entity(e).insert(<el/>)` lands on `e` | `insert_element_lands_on_the_entity` | audit: `spawn(<el/>)` |
| `spawn_batch` | `commands.spawnBatch(list)` | `spawn_batch_returns_ids_in_order` | audit: commands |
| `insert_batch` | `commands.insertBatch(list)` | `insert_batch_inserts_each` | research |
| `insert_if_new`, `try_insert` | `insertIfNew`, `tryInsert` | `insert_if_new_and_try_insert` | audit: commands |
| `retain`, `clear` | `retain(...)`, `clear()` | `retain_and_clear` | audit: commands |
| `remove_with_requires` | `removeWithRequires(...)` | `remove_with_requires` | audit: commands |
| `clone_and_spawn` | `cloneAndSpawn()` | `clone_and_spawn_copies_components` | audit: commands |
| `insert_recursive`, `remove_recursive` | `insertRecursive`, `removeRecursive` | `insert_and_remove_recursive` | research |
| `remove` | `remove(...)` | `remove_takes_only_named` | audit: commands (untested) |
| Spawns per run (no limit) | pool refilled on demand | `a_thousand_spawns_in_one_run` | audit: 256 spawns |
| `despawn` with linked children | `despawn()` | `despawn_takes_children` | audit: despawn recursion |
| `EntityEvent` propagate | `static propagate = true` | `entity_event_bubbles_until_stopped` | audit: bubbling |
| `On<Discard, C>` | `On<Discard<C>>` | `discard_sees_the_old_value` | audit: lifecycle |
| `On<Despawn, C>` | `On<Despawn<C>>` | `despawn_observers_run_in_bevy_order` | audit: lifecycle |
| `Remove`/`Discard`/`Despawn` read the value | the value copied at trigger time | `remove_observer_reads_the_value` | research |
| Component hooks | declined: hooks run inside the structural change, where JS cannot run; observers with the copied value cover the need | — | audit: hooks |
| Required components (`#[require]`) | `static require = [...]` | `required_components_from_every_path` | audit: required |
| `Disabled`, `DefaultQueryFilters` | queries skip disabled entities | `probe_disabled_and_sparse_filters` | audit: `Disabled` |
| Sparse-set components in queries | filters match as Bevy; data refused | `probe_disabled_and_sparse_filters`, `sparse_set_data_is_refused` | research |
| `Allow<T>` | declined for now: `Has<Disabled>` covers it (`Disabled` has no layout, so `Option<Disabled>` is not query data) | — | research |
| `SpawnDetails`, `sort`, `entry`, `queue` | declined: JS covers them (`Array.sort`), or no TS need | — | research |

#### Scenario: Each row's test passes
- **WHEN** `cargo test -p bevy-ts-cli --test ecs_queries` runs
- **THEN** every test named in the table exists and passes

#### Scenario: A declined item
- **WHEN** a script writes `Query<[SpawnDetails]>`
- **THEN** loading fails naming `SpawnDetails` as not query data, with the file and line
