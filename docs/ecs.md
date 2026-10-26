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
the rows that pass; `With`/`Without` on table components decide whole tables. In raw `q.tables`,
a table with `rows` set matches only those rows (`len` is the table's length), and a query naming a
sparse-set component hands a table once per matched archetype.

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
"`Spark` is a sparse-set component; it can be a filter but not query data". As a filter
(`With`, `Without`, `Changed`, `Added`, inside `Or`) or as `Has`, it matches what Bevy's query
matches. A sparse-set component splits no tables, so such a query is run by archetype: each
matched archetype hands JS a row list into its table, as `Changed` does. A query of table
components only keeps the whole-table path. A `table.write(T)` marks only the rows the query
matched.

## Heap fields: `string` and `T[]`

A TS component may hold a string or a list, as a Bevy component holds a `String` or a `Vec<T>`:

```ts
export class Tag extends Component { label: string = "crate"; scores: u32[] = []; declare note: string }
```

- A list's items are a number kind (`f32`, `f64`, `i8`..`u32`), `bool` or `string`. Vectors,
  `Entity`, objects and nested lists fail the load with the file and line. A `declare`d field
  starts as `""` or `[]`.
- Components only: a heap field in a TS resource, a message, an event or a `Local` fails the load
  ("heap fields (string, T[]) are allowed on components only").
- The value lives in the component's own bytes, Rust's `String` or `Vec<T>`, so a Rust system reads
  it as Bevy would. A component with one starts with an 8-byte header Rust owns (the layout's
  offsets skip it), registers a drop fn and deep-copies on `cloneAndSpawn`. A component with no
  heap field is laid out, dropped and cloned as before.
- A row reads a heap field through the host, cached by the row's changed tick: the first read of a
  row costs a host call, later reads of an unchanged row cost none. A list reads as a frozen
  `readonly T[]`; `tag.scores.push(1)` throws a `TypeError`.
- A `Mut<Tag>` row assigns a whole field: `tag.label = "open"`, `tag.scores = [...tag.scores, 4]`.
  The write is a host call that applies at once and marks the row changed, so the same system,
  a later Rust system and `Changed<Tag>` see it this frame.
- `Tag.of({ label: "lamp" })` spawns and inserts carry the values in the same command; a value the
  command leaves out takes its default, built in Rust for each entity.
- `q.tables` gives no raw arrays for a component with a heap field; read it on `q.each` rows.
  Its plain fields stay columns on those rows.
- An observer's lifecycle value (`Remove`, `Despawn`, `Discard`) reads a heap field live, so the
  read throws once the component is gone; copy what you need while it is there.

What a row costs, in ns, 1,000 rows, against a column (`bench/ts-heap-fields`, release, medians of
8 rounds, `results/runs.txt`):

| Per row | Column (`f32`) | `string` | 8-item `f32[]` |
| --- | --- | --- | --- |
| Read, unchanged row (cached) | 8 | 29 | 30 |
| Read, changed row (host call) | 8 | 109 | 110 |
| Write on `Mut` (host call) | not measured (a typed-array store) | 381 | 397 |
| Despawn | 21 | 38 (the whole row, both fields) | |

A plain field of a heap component still reads as a column (8 ns). A component with no heap field
matches main: column read 8.5 on main against 8 here, despawn 22 against 21.

Saves, TSX props, `.bsn` levels (strings; a list in a level is refused) and the protocol carry heap
fields as JSON strings and arrays (docs/saves.md, docs/reflection.md).
## Required components

```ts
export class Enemy extends Component { static require = [Health.of({ hp: 10 }), Shield] }
```

As Rust's `#[require(Health = ..., Shield)]`. Inserting `Enemy` from any path (commands, TSX, a
level, a Rust insert) inserts each listed component the entity lacks, with the given fields and
the rest defaults; one it has keeps its value. Requirements apply transitively. A listed class is
a TS component or a Rust component with a native layout, declared before the class that lists
it. A cycle fails the load naming the classes. `removeWithRequires` removes them too.

They land one flush after Bevy's would: bevy-ts inserts them from the component's `on_add` hook,
through the world's commands, not in the same bundle. They are there before the next system.

## Commands

```ts
const e = commands.spawn(Transform.of({ translation: [1, 0, 0] }), <entity><mesh shape="cuboid" /><Health hp={3} /></entity>);
```

**Breaking:** `spawn` with an element returns the element's root entity, as Bevy's `spawn_scene`
returns the scene's root. Components passed beside the element go on that root, not on a new
parent. Before, the element was mounted as a child of a new entity. A fragment (`<>...</>`)
throws: spawn each, or wrap them in `<entity>`. A component given beside the element and on its
root throws. `commands.entity(e).insert(<el/>)` puts the root's components on `e` and its
children under `e`. An entity tsx made (a scene's, or one `spawn(<el/>)` returned) refuses `insert(<el/>)`: mount
the element under it, or insert components.

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
`spawn` and `insert` take any component value. `spawnBatch`, `insertBatch`, `insertIfNew`,
`tryInsert` and `insertRecursive` take layout values only: a reflected value, or one that sets
fields the layout lacks, throws naming the method.
`clear` and `retain` remove every game component but keep bevy-ts's own markers (what made the
entity), and `cloneAndSpawn`'s clone is a TS spawn: a restart despawns it.

## Observers

`On<Add<C>>`, `On<Insert<C>>`, `On<Discard<C>>`, `On<Remove<C>>` and `On<Despawn<C>>`. TS
observers run at the next stage boundary, after the change, so for `Discard`, `Remove` and
`Despawn` Rust copies `C`'s value when the event fires and `on` gives its fields
(`on.hp`) as they were. A despawn runs `Despawn`, `Discard`, `Remove`, in Bevy's order. A Rust
component without a layout gives `on.entity` only.

An `EntityEvent` with `static propagate = true` runs for its target, then each ancestor up
`ChildOf`, until an observer calls `on.propagate(false)`.

There are no TS component hooks: a hook runs inside the structural change, where JS cannot run.
Observers with the copied value cover what a hook would do. bevy-ts's own `on_add` hook on every
TS component inserts its required components.
