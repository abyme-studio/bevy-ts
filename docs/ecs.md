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
once), `q.iterAncestors(e)` (with `ChildOf` data: parent first, up to the root),
`q.iterDescendants(e)` (with `Children` data: children, then theirs, breadth first; not `e`),
and `q.tables` for raw typed arrays.

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

A TS component or resource may hold a string or a list, as a Bevy component or resource holds a
`String` or a `Vec<T>`:

```ts
export class Tag extends Component { label: string = "crate"; scores: u32[] = []; declare note: string }
export class Title extends Resource { text: string = "untitled" }
export class Chat extends Message { text: string = ""; to: u32[] = [] }
```

- A list's items are a number kind (`f32`, `f64`, `i8`..`u32`), `bool`, `string`, a vector
  (`Vec2`, `Vec3`, `Vec4`, `Quat`), `Entity`, a string-literal union, or an object type of those
  but `string` (`{ at: Vec3; by: Entity; kind: Kind; hp: f32 }[]`, a literal, an alias or an
  interface). A nested list (`f32[][]`), a `string` or list member of an object, an optional
  member and an object field outside a list fail the load with the file and line. A `declare`d
  field starts as `""` or `[]`.
- A row reads a vector item as a frozen `Vec3` (a `Vec2` with z 0) or `Quat` (`Vec4` too), an
  entity as `Entity`, a union as its name and an object as a frozen object of its members. An
  object item, written, gives every member; a list item is replaced, never edited member by member
  (`hits[0] = { ...hits[0], hp: 2 }`).
- A Rust system reads a list of them as Bevy keeps one: `Vec<Vec2>`, `Vec<Vec3>`, `Vec<Entity>`,
  `Vec<[f32; 4]>` for `Vec4` and `Quat` (glam's are 16-aligned, a column's are not), `Vec<u8>`
  (or `u16`, `u32`, by the union's size) of the variant index, and an object as a `Vec<T>` of a
  `#[repr(C)]` struct of its members in order. It may push onto one: the list is freed with the
  layout Rust grew it with.
- Components, resources, `Local`, messages and events take them; an `In<E>` input of an event
  with one does not yet (`runSystemWith` throws).
- A `Local<{ log: string; hits: u32[] }>` keeps its heap values across runs in the system's
  memory, Rust's `String` and `Vec<T>` as in a component. A reload that keeps a field moves its
  value; a changed field takes its default. A read is cached until the system writes that field
  (the system is the `Local`'s only writer).
- The value lives in the component's own bytes, Rust's `String` or `Vec<T>`, so a Rust system reads
  it as Bevy would. A component with one starts with an 8-byte header Rust owns (the layout's
  offsets skip it), registers a drop fn and deep-copies on `cloneAndSpawn`. A component with no
  heap field is laid out, dropped and cloned as before.
- A row reads a heap field through the host, cached by the row's changed tick: the first read of a
  row costs a host call, later reads of an unchanged row cost none. On a read row a list is a
  frozen `readonly T[]`; `tag.scores.push(1)` throws a `TypeError`.
- A `Mut<Tag>` row assigns a whole field: `tag.label = "open"`, `tag.scores = [...tag.scores, 4]`.
  The write is a host call that applies at once and marks the row changed, so the same system,
  a later Rust system and `Changed<Tag>` see it this frame.
- On a `Mut<Tag>` row, `ResMut` or `Local` a list is also a plain array the system edits in place:
  `tag.scores.push(4)`, `tag.scores[0] = 1`, `sort`, `length = 2`. It is the run's own copy, the
  same array all run. At the end of the run (or before another accessor in the same run reads that
  row's field) it is compared with what it was copied from, items by `Object.is` (vectors lane by
  lane, objects member by member, so a `NaN` written over a `NaN` is no change), and sent as item
  edits: a truncate, a set per changed item, an
  append. When more than half the items changed (a `sort`, a `shift`) the whole list is sent
  instead. Rust applies them to its `Vec` in place and marks the row changed once. A list read
  and left unchanged sends nothing and marks nothing, as Bevy's `set_if_neq`.
- The same system sees its edits at once (it holds the array); a later Rust system, a later TS
  system and `Changed<Tag>` see them after the run. An observer's edits are sent as a system's.
  An assignment drops the edits made before it in that run; edits after it are compared with it.
  An array kept past its run is no longer the row's: editing it writes nothing.
- An item left unset at the end of the run (`a[10] = x` past the end, `length = 9` grown and not
  filled) fails the run with a `TypeError` naming the field and the first such index, and writes
  nothing for that list.
- `Tag.of({ label: "lamp" })` spawns and inserts carry the values in the same command; a value the
  command leaves out takes its default, built in Rust for each entity.
- `q.tables` gives no raw arrays for a component with a heap field; read it on `q.each` rows.
  Its plain fields stay columns on those rows.
- A resource's heap field reads and writes as a component's: `Res<Title>` reads `title.text`,
  cached by the resource's changed tick, `ResMut<Title>` assigns it whole. `resourceEquals`
  compares it by value.
- An observer's lifecycle value (`On<Remove<Tag>>`, `Despawn`, `Discard`) is a copy of the value
  taken when the event fired, heap fields included, so `on.label` reads what the component held
  then, after it is gone. Bevy runs `On<Remove>` while the component is still on the entity; a TS
  observer runs at the next stage boundary, so the copy is the closest match. The copy is freed
  once, after the event's last observer. This is a behaviour change from ts-component-heap-fields,
  where the read went to the wrong place (a resource) and always threw.
- A message or event's heap field is read-only, as Bevy's `MessageReader` and `On` give `&M`
  and `&E`: `m.text` reads it through the host, cached per message; assigning it throws a
  `TypeError`. `MessageWriter.write(Chat.of({ text }))` carries it in the command, and a
  `w.next()` view keeps what is set on it (`m.text = "hi"`) and sends it with the record. Each
  value is built in Rust, owned by its queue or pending event and freed once: when Bevy's
  two-frame rule expires the message, after the event's last observer, on a restart and when the
  world drops. A reload keeps unread messages; a failed one copies them and drops the copy.
- `plugin_api::write_message` (a Rust port writing a TS message) now gives each field it leaves
  out its declared default, as a TS write does; it used to write zero.

Every heap read and write names a place: the row's entity, the resource, the running system's
`Local`, a message by its sequence number, or the running observer's value. `crates/bevy-ts/src/places.rs` is the one owner of that model (the host call's
slot layout and the `RunPlace` the runner sets per run); `reflect_codec::call` is its one reader.
A place that is not live (no observer running) fails the call naming it.

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

Resources and lifecycle values (same bench, medians of 8 rounds against main at 2ba1c6ad, load
about 45): a plain `Res` read is 8 ns on main and 8.5 here, a plain despawn 20 against 21, within
the bench's 1 ns step. A heap resource's string read while unchanged is 11 ns (cached by the
resource's changed tick). An `On<Remove>` observer reading the removed value's string costs 267 ns
an event over one that reads nothing: the trigger-time copy, its free and one host call.

A `Local` (3 rounds against main at 913e0428c, load about 45): a plain `Local` read is 8 ns on
main and here; a `Local` string read is 8 ns, cached until the system writes it.

Messages and events (8 rounds against main at 7f41da5ca, load about 170, so rounds spread by a
third; ns a message or event over a whole frame, written or triggered, then read by one system or
observer): a plain message is 810 on main and 804 here; a plain event 3290 on main and 3462 here,
5% apart with the two ranges (2835-4875, 3101-5126) overlapping. A string field costs about 1,230
ns a message (2033) and 1,790 an event (5249): the string's build from JS bytes, its free and one
host call a read.

A list of vectors (3 rounds against main at f0f02fa5c, load about 90, medians): an 8-item
`Vec3[]` reads in 73 ns a row unchanged and 647 changed, against 60 and 237 for an 8-item `f32[]`
in the same rounds: a changed read builds 8 frozen `Vec3`s. The plain arms match main in those
rounds (column 15 against 16, despawn 33 against 38, message 901 against 944, event 3418 against
3469).

In-place edits against a whole assignment (3 rounds against main at 75757677f, load about 30,
medians, ns a row for the whole run: the read, the edit and its host call):

| Per row | 8-item `f32[]` | 64-item `f32[]` |
| --- | --- | --- |
| `push` or `pop`, in place | 1124 | 1854 |
| `push` or `pop`, whole assignment | 1161 | 3847 |
| `[3] =`, in place | 940 | 1893 |
| `[3] =`, whole assignment | 1240 | 3548 |

At 8 items the two are within noise for a push (869-1212 against 1021-1447); at 64 an edit costs
about half an assignment, which re-sends every item. The plain arms match main in those rounds
(column 12 against 12, despawn 26 against 29, message 607 against 725, event 2228 against 2559).

The ts-heap-fields-2 verdict (8 rounds each, `results/runs.txt`):

- Plain arms against main at 740badaa0 (load about 50, medians, ns): column 18.5 on main and
  20 here, despawn 42 and 43, `Res` read 19 and 19, `Local` read 20 and 20.5, a message 1080 and
  1066, an event 4048 and 4158, each pair's ranges overlapping. No plain type is slower.
- Stage 1's arms against stage 1's own build (b229f74c3), interleaved (`vs.sh`, process
  `ts-heap-fields-vs-ref`). At load about 100 (medians): column 8.5 there and 8 here, string hit
  28 and 30.5, miss 116.5 and 111.5, write 366 and 338.5, 8-item `f32[]` hit 31 and 32, miss 119
  and 107.5, despawn of both fields 40 and 43. The list write was 398.5 against 514.5, ranges
  apart (343-488, 504-674): every assignment built a pending-edit key it had no use for. With
  that skipped when no edit is pending, a rerun at load 85-233 has the list write at 352 there
  and 361 here (fastest of 8; that load spreads medians by 2x).
- At 8 items `push` in place and a whole assignment stay within noise over 8 rounds (1780 against
  2103, ranges 1399-2243 and 1574-2415); at 64 an edit is about half (2873 against 5385).

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
A subclass does not inherit its base's `require`.

On a hot reload, a new or changed `require` touches no entity that already has the component; it
applies to inserts from then on. A layout change to a component re-adds it to its entities, which
runs its `on_add` again: a required part an entity had removed comes back.

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
`entity(e)`, `trigger`, `write`, `spawnNative`, `mount`, `unmount`, `delayed()` (commands that run
later on the game clock: docs/time.md, "Delayed commands").

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
