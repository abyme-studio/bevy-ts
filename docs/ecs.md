# Queries, commands and lifecycle

What a TS system can say about the ECS, under Bevy 0.20's names. Each item and the test that
proves it is listed in the `ecs-coverage` spec (`openspec/specs/ecs-coverage/spec.md`).

## Queries

```ts
export function hurt(q: Query<[Entity, Mut<Health>, Has<Shield>], [With<Enemy>, Without<Player>]>) {
  q.each((e, health, shielded) => { if (!shielded) health.hp -= 1; });
}
```

Data, in the tuple:

| TS | Bevy | What a row gives |
|---|---|---|
| `C` | `&C` | a read-only view; writing throws |
| `Mut<C>` | `&mut C` | a writable view; a write marks the row changed |
| `Option<C>` | `Option<&C>` | the view or `undefined` |
| `Ref<C>` | `Ref<C>` | a read-only view with `isAdded()`, `isChanged()`, `lastChanged()` |
| `Has<C>` | `Has<C>` | a `boolean`; never filters, no access to `C` |
| `AnyOf<[A, B]>` | `AnyOf<(&A, &B)>` | `[A \| undefined, B \| undefined]`; at least one is there |
| `Entity` | `Entity` | the entity |

Filters, alone or in a tuple (all of):

| TS | Bevy |
|---|---|
| `With<C>`, `Without<C>` | `With<C>`, `Without<C>` |
| `Changed<C>`, `Added<C>` | since this system last ran |
| `Spawned` | the entity was spawned since this system last ran |
| `Or<[F1, F2]>` | any of: `Or<[Changed<Health>, Added<Shield>]>` |

`Changed`, `Added` and `Spawned` (also inside an `Or`) are checked per row in Rust, which hands JS
the rows that pass; `With`/`Without` decide whole tables.

### Query methods

`q.each(fn)` is the hot loop: reused views, no iterator protocol. Also: `for (const row of q)`,
`q.len`, `q.isEmpty()`, `q.single()` (throws unless one), `q.get(e)`, `q.contains(e)`,
`q.iterMany(list)` (rows of the listed entities that match, in list order), `q.getMany(list)`
(throws naming the first that does not match), `q.iterCombinations(k)` (each set of `k` rows
once), `q.iterAncestors(e)` (with `ChildOf` data: parent first, up to the root), and `q.tables`
for raw typed arrays.

### Params that skip the system

- `Single<[data], filter>`: the one matching row, as a tuple. With none or several the system is
  not called, as Bevy's param validation skips it: no JS call, no error.
- `Option<Single<...>>`: the row or `undefined`; skipped with several.
- `Populated<[data], filter>`: a query with at least one row; skipped when empty.

Rust counts the rows (after `Changed`/`Added`/`Spawned`) before calling JS.

### Other params

- `RemovedComponents<C>`: entities that lost `C`, by `remove` or despawn, since this system last
  read it. `each`, iteration, `len`, `isEmpty()`. Its cursor survives a reload.
- `SystemChangeTick`: `thisRun` and `lastRun`.

### Disabled entities

TS queries honour `DefaultQueryFilters`: an entity with `Disabled` matches only a query that names
`Disabled` (`With<Disabled>`, `Has<Disabled>`). The filters are read when a generation loads.

### Sparse-set components

A Rust component with `#[component(storage = "SparseSet")]` cannot be query data: its storage is
not in table order, so it cannot be a zero-copy column. Loading fails with
"`Spark` is a sparse-set component; it can be a filter but not query data". As a filter or
`Has`, it fails the load too until queries take the sparse-set path.

## Commands

```ts
const e = commands.spawn(Transform.of({ translation: [1, 0, 0] }), <entity><mesh shape="cuboid" /><Health hp={3} /></entity>);
```

**Breaking:** `spawn` with an element returns the element's root entity, as Bevy's `spawn_scene`
returns the scene's root. Components passed beside the element go on that root, not on a new
parent. Before, the element was mounted as a child of a new entity. A fragment (`<>...</>`)
throws: spawn each, or wrap them in `<entity>`. A component given beside the element and on its
root throws. `commands.entity(e).insert(<el/>)` puts the root's components on `e` and its
children under `e`.

`spawn` returns a real entity at once. There is no limit per run: the id pool refills with one
host call per 256 spawns past the first 256.

`commands`: `spawn`, `spawnBatch(list)` (one op, entities in order), `insertBatch([[e, ...parts]])`,
`entity(e)`, `trigger`, `write`, `spawnNative`, `mount`, `unmount`.

`commands.entity(e)`, each Bevy's `EntityCommands` method of the same name, applied in order
after the run:

| TS | Bevy |
|---|---|
| `insert`, `insertIfNew`, `tryInsert` | `insert`, `insert_if_new`, `try_insert` |
| `remove`, `removeWithRequires`, `retain`, `clear` | same names |
| `insertRecursive`, `removeRecursive` | down `Children` |
| `despawn` | with its children |
| `cloneAndSpawn()` | returns the new entity at once |
| `setParent`, `addChildren`, `withChildren((p) => p.spawn(...))`, `detachChildren`, `despawnChildren` | `ChildOf`, `add_children`, `with_children`, `detach_all_children`, `despawn_children` |

A command on a gone entity logs a warning naming the method and entity; `tryInsert` stays quiet.

## Observers

`On<Add<C>>`, `On<Insert<C>>`, `On<Discard<C>>`, `On<Remove<C>>` and `On<Despawn<C>>`. TS
observers run at the next stage boundary, after the change, so for `Discard`, `Remove` and
`Despawn` Rust copies `C`'s value when the event fires and `on` gives its fields
(`on.hp`) as they were. A despawn runs `Despawn`, `Discard`, `Remove`, in Bevy's order. A Rust
component without a layout gives `on.entity` only.

An `EntityEvent` with `static propagate = true` runs for its target, then each ancestor up
`ChildOf`, until an observer calls `on.propagate(false)`.

There are no component hooks: a hook runs inside the structural change, where JS cannot run.
Observers with the copied value cover what a hook would do.
