## Why

A TS system cannot say most of what a Bevy system says about queries, commands, lifecycle and
hierarchy: the audit (`docs/research/bevy-coverage.md`, ECS rows owned by `ecs-queries`) found all
13 rows missing or partial, and research (`docs/research/ecs-queries.md`) found two bugs the audit
missed: a sparse-set component in a TS query returns wrong rows, and a TS `Remove` observer runs
after the value is gone.

## What Changes

- Sparse-set components in TS queries. First a plain fix: a query naming a sparse-set component
  (filter, data or `Has`) fails the load with a clear error, instead of returning wrong rows.
  Then, in the ask-first stage, filters on sparse-set components work as in Bevy (non-dense
  queries get a per-table row list; `Changed`/`Added` read per-entity sparse ticks); sparse-set
  query data stays refused. Dense queries keep today's path.
- Filters and query data: `Or<[...]>`, `Spawned`, `Has<C>`, `AnyOf<[...]>`, `Ref<C>`.
- System params: `Single<D, F>` and `Populated<D, F>` skip the system in Rust when they do not
  match (`Option<Single<…>>` for zero or one); `RemovedComponents<C>`; `SystemChangeTick`.
- Query methods: `iterMany`, `getMany`, `contains`, `isEmpty`, `iterCombinations`,
  `iterAncestors`, `iterDescendants`.
- Lifecycle: `On<Discard<C>>` and `On<Despawn<C>>`. `Remove`, `Discard` and `Despawn` observers get
  the value as it was when the event fired (copied at trigger time), since TS observers run
  deferred. No component hooks: observers cover the TS need (decision in design.md).
  `Discard`/`Despawn` and the hooks decision come to this change from rust-events, whose
  non-goals hand them over (`rust-events/design.md`).
- Commands: **BREAKING** `spawn(<el/>)` returns the element's root entity, and components passed
  beside an element go on that root instead of a new parent; `entity(e).insert(<el/>)` puts the
  element's root components on `e` instead of mounting a child. A fragment is refused in both.
  New: `spawnBatch`, `insertBatch`, `insertIfNew`, `tryInsert`, `retain`, `clear`,
  `removeWithRequires`, `cloneAndSpawn`, `insertRecursive`, `removeRecursive`, `withChildren`,
  `addChildren`, `detachChildren`, `despawnChildren`. The 256-spawns-per-run limit goes.
- Tests for what works but is untested: `Disabled` filtering, despawn recursion, `remove`,
  `EntityEvent` bubbling.
- Ask-first stage, last and separate so the rest lands without it: required components on TS
  components (`static require = [...]`) and the sparse-set query support above. Custom
  relationships are a change of their own, custom-relationships, after this one.
- A coverage table (`ecs-coverage`) with one row per item and the test that proves it.

## Capabilities

### New Capabilities
- `ecs-coverage`: the table of Bevy 0.20 ECS query, command, lifecycle and hierarchy items, the TS
  form of each, and the test that proves it.

### Modified Capabilities
- `ts-systems`: param kinds, the iteration API, row lists, the command buffer, plus new
  requirements for `Single`/`Populated`, `RemovedComponents`, `Discard`/`Despawn` with values,
  sparse-set components, hierarchy commands and walks, `Disabled` and required components.

## Impact

- Code: `crates/bevy-ts/src/systems.rs` (params, rows, skip, sparse sets), `commands.rs` (new ops,
  pool), `observers.rs` and `manifest.rs` (lifecycle kinds, value copy), `components.rs`
  (required components), `runtime.rs` (pool refill host call);
  `crates/bevy-ts-cli/src/reader.rs` (new param, data and filter kinds);
  `crates/bevy-ts-tsx/src/mount.rs` (mount at a given root entity);
  `packages/bevy-ts/runtime/core.js` and `index.d.ts`; tests in
  `crates/bevy-ts-cli/tests/ecs_queries.rs`; `docs/ecs.md`.
- Neighbours: reflect-coverage agreed its `cuboid` migration row drops "the mesh is a child"
  (design.md, decision 9). `insertRecursive(Disabled)` needs reflect-coverage stage 2;
  `iterDescendants` needs its stage 3 (`Children` as query data). ecs-schedules shares the Rust
  skip path with `Single`/`Populated`.
- Has the operator's go, given 2026-10-22 (stage 7 of tasks.md):
  - required components on TS components: a bevy-ts-owned `on_add` hook by component id, a new
    architectural pattern;
  - sparse-set filter support: changes the per-frame query path for non-dense queries
    (performance), landed with a bench line.
- Moved out: custom relationships, to the custom-relationships change (design.md, decision 12).
- No new dependency.
