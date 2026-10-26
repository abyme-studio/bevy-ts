## MODIFIED Requirements

### Requirement: System param kinds
The `bevy-ts` module SHALL export these param types, each mapping to Bevy's own:

- `Query<[data...], filter>`: data is a component class (read), `Mut<C>` (write), `Option<C>` or `Option<Mut<C>>` (the row's value or `undefined`), `Ref<C>` (read, plus `isAdded()`, `isChanged()` and `lastChanged()`), `Has<C>` (a `boolean`, no access to `C`, never filters), `AnyOf<[A, B, ...]>` (each as `Option`, at least one present) and `Entity`. The filter is `With<C>`, `Without<C>`, `Changed<C>`, `Added<C>`, `Spawned` (the entity was spawned since this system last ran), `Or<[F1, F2, ...]>` (any of) or a tuple of them (all of). `Changed<C>` and `Added<C>` match rows whose tick is newer than this system's last run.
- `Single<[data...], filter>`: exactly one match, given as that row; `Option<Single<...>>`: zero or one, the row or `undefined`. `Populated<[data...], filter>`: a `Query` with at least one match. See "Single and Populated skip the system".
- `RemovedComponents<C>`: entities that lost `C` since this system last read it (see "RemovedComponents").
- `SystemChangeTick`: `thisRun` and `lastRun`, this run's tick and the system's last-run tick.
- `Res<R>` and `ResMut<R>`: a resource, read-only or writable.
- `Local<{ field: type, ... }>`: per-system state with a layout (field types `f32`, `u32`, ... as in ts-components), kept in Rust under the system's identity, so it survives reloads (hot-reload).
- `Commands`: the system's command buffer.
- `MessageReader<M>` and `MessageWriter<M>`: a message type's queue (see "Messages").
- `NextState<S>` and `State<S>`: a TS state machine (see "States").
- `In<E>`: a one-shot system's input, first param only, `E` an `Event` class (ts-schedules).
- Param kinds that native ports register (for example `ReadRapierContext`), declared in their types package and known to the compile step.

A param type that is none of these, an alias of one, or an object type whose properties are these (a param bundle, Bevy's `#[derive(SystemParam)]`) SHALL fail the load with its file and line.

#### Scenario: A param bundle
- **WHEN** a system takes `{ score, time }: Scoring` where `interface Scoring { score: ResMut<Score>; time: Res<Time> }`
- **THEN** it receives an object with a writable `score` and a read-only `time`

#### Scenario: In anywhere but first
- **WHEN** a registered one-shot system takes `In<SpawnAt>` as its second param
- **THEN** loading fails with "In<SpawnAt> must be the first param", its file and line

#### Scenario: Or of two row filters
- **WHEN** a system has `Query<[Entity], Or<[Changed<Health>, Added<Shield>]>>`, and since its last run one entity's `Health` changed and another got `Shield`
- **THEN** it visits exactly those two entities

#### Scenario: Has
- **WHEN** a system has `Query<[Entity, Has<Stunned>]>` over three entities, one with `Stunned`
- **THEN** it visits all three, and the second element is `true` for that one and `false` for the others

#### Scenario: Spawned
- **WHEN** a system with `Query<[Entity], [With<Enemy>, Spawned]>` runs after another system spawned two `Enemy` entities
- **THEN** it visits those two, and none of them on its next run

#### Scenario: AnyOf
- **WHEN** a system has `Query<[AnyOf<[Health, Shield]>]>` over entities with `Health` only, `Shield` only, both, and neither
- **THEN** it visits the first three, with `undefined` for the missing part, and not the fourth

#### Scenario: Ref
- **WHEN** a system has `Query<[Ref<Health>]>` and another system wrote `Health` on one row since its last run
- **THEN** that row's `isChanged()` is `true`, every other row's is `false`, and writing through the `Ref` view throws

#### Scenario: SystemChangeTick
- **WHEN** a system takes `ticks: SystemChangeTick` and runs twice
- **THEN** the second run's `ticks.lastRun` equals the first run's `ticks.thisRun`

### Requirement: Iteration API
A query param SHALL offer `q.each((...row) => ...)`, where each element of `row` is the entity or a reused accessor object positioned on the current entity (the hot loop: no iterator protocol), iteration with `for (const [...] of q)`, `q.single()` (exactly one match, or throws), `q.get(entity)` (the row or `undefined`), `q.contains(entity)`, `q.isEmpty()`, `q.iterMany(entities)` (the rows of the listed entities that match, in list order, skipping the rest), `q.getMany(entities)` (an array of rows, throwing on the first entity that does not match), `q.iterCombinations(k)` (every set of `k` distinct rows, once each), `q.len`, `q.tick` (the run's change tick) and `q.tables`, each with `len`, `entities`, `read(T)` giving a component's typed arrays and offsets, `write(T)` giving the same for a written component and marking every row of that table changed when the run ends, and `split(T)` giving the arrays plus the column's `ticks` (`Uint32Array`) and leaving marking to the caller. A query whose data has `ChildOf` SHALL offer `q.iterAncestors(entity)` (parent first, up to the root), and one whose data has `Children` SHALL offer `q.iterDescendants(entity)` (breadth first), as Bevy's `Query<&ChildOf>` and `Query<&Children>` do. The frame's times SHALL come from `Res<Time>` (see "Time as a resource view").

#### Scenario: each and tables agree
- **WHEN** the same system body is written once with `q.each` and once with `q.tables`
- **THEN** both produce identical component values after one frame

#### Scenario: split marks only what the caller marks
- **WHEN** a system takes `split(Mover)` on a table, writes rows 3 and 7 and sets `ticks[3] = q.tick` and `ticks[7] = q.tick`
- **THEN** after the run only rows 3 and 7 of that table read as changed

#### Scenario: iterMany keeps list order and skips misses
- **WHEN** a system calls `q.iterMany([c, x, a])` where `a` and `c` match and `x` does not
- **THEN** it yields `c`'s row, then `a`'s; `q.getMany([c, x, a])` throws naming `x`

#### Scenario: Pairs
- **WHEN** a query matches 4 entities and the system calls `q.iterCombinations(2)`
- **THEN** it yields 6 pairs, each of two distinct entities, no pair twice

#### Scenario: Ancestors
- **WHEN** `c`'s parent is `b`, `b`'s parent is `a`, and a system with `Query<[ChildOf]>` calls `q.iterAncestors(c)`
- **THEN** it yields `b`, then `a`

### Requirement: Changed and Added rows reach JS as row lists
For a query with a row filter (`Changed<C>`, `Added<C>`, `Spawned`, or an `Or` holding one), Rust SHALL evaluate the filter per matched row against the system's last-run tick, kept per system identity, and SHALL hand JS the matching row indices per table. `Changed<C>` and `Added<C>` SHALL read `C`'s ticks; `Spawned` SHALL read the entity's spawn tick. Inside an `Or`, a `With`/`Without` term SHALL hold for every row of a table or none. `each`, iteration, `get`, `single` and the query methods SHALL visit only those rows; `table.rows` SHALL expose them, and `table.read`, `write` and `split` SHALL still cover every row.

#### Scenario: Only changed rows
- **WHEN** a Rust system changes `Health` on 2 of 1,000 entities and a TS system with `Query<[Health], Changed<Health>>` runs next
- **THEN** its `each` visits exactly those 2 rows

### Requirement: Structural changes go through the command buffer
The `Commands` param SHALL offer `spawn(...parts)`, `spawnBatch(list)`, `insertBatch(list)`, `entity(e)` (see "Entity commands"), `trigger(event)` (an entity event names its target in its `entity` field; see "Observers"), `write(message)` and `spawnNative(name, params)`. A part SHALL be a component value such as `Health.of({ hp: 3 })`, a component class (its defaults), or an element. Commands SHALL be encoded by JS into a binary arena owned by Rust, with component values in their native layout, and applied by Rust after the system returns, before the next system runs. `spawn` SHALL return a real `Entity` at once, taken from a pool of ids allocated with `EntityAllocator::alloc`; when the pool runs out, JS SHALL refill it with one host call, so a run MAY spawn any number of entities.

`spawn` with an element SHALL spawn the element's root at the returned entity, with the element's children as its children, as Bevy's `World::spawn_scene` returns the scene's root; components passed beside the element SHALL go on that root. An element whose root is not one entity (a fragment) SHALL throw at the call, naming the fix (spawn each, or wrap them in `<entity>`). A component given both beside the element and on its root SHALL throw at the call, as a component given twice on one entity does in TSX.

`spawnBatch(list)` SHALL take a list of part lists, encode one op and return the new entities in list order. `insertBatch(list)` SHALL take a list of `[entity, ...parts]` and insert each, as Bevy's `insert_batch`.

#### Scenario: Spawning and using the id in the same run
- **WHEN** a system spawns an entity and then calls `commands.entity(child).setParent(spawned)` in the same run
- **THEN** after the run both entities exist and the child's parent is the spawned entity

#### Scenario: The arena overflows
- **WHEN** a run queues more command bytes than the arena holds
- **THEN** the overflow is kept on the JS side and applied after the run, with no command lost and no allocation in Rust during the run

#### Scenario: Spawning an element returns its root
- **WHEN** a system calls `const e = commands.spawn(Transform.of({ translation: [1, 0, 0] }), <entity><mesh shape="cuboid" /><Health hp={3} /><entity><Name value="tip" /></entity></entity>)`
- **THEN** after the run `e` has `Transform`, `Mesh3d` and `Health`, it has no parent from the spawn, and its one child has `Name` "tip"

#### Scenario: A fragment
- **WHEN** a system calls `commands.spawn(<><entity /><entity /></>)`
- **THEN** the call throws naming `<entity>`, and nothing is spawned

#### Scenario: A thousand spawns in one run
- **WHEN** a system spawns 1,000 entities in one run
- **THEN** all 1,000 exist after the run, each id distinct, and JS made at most 4 refill calls into Rust

#### Scenario: spawnBatch
- **WHEN** a system calls `commands.spawnBatch([[Enemy.of()], [Enemy.of(), Alive.of()]])`
- **THEN** it returns two entities, and after the run the second has `Alive` and the first does not

## ADDED Requirements

### Requirement: Entity commands
`commands.entity(e)` SHALL offer these methods, each with the semantics of Bevy 0.20's `EntityCommands` method of the same snake-case name, applied in order with the rest of the run's commands:

- `insert(...parts)`, `insertIfNew(...parts)` (keeps a value already there), `tryInsert(...parts)` (does nothing if `e` is gone);
- `remove(...classes)`, `removeWithRequires(...classes)`, `retain(...classes)` (removes every other component), `clear()` (removes every component, keeps the entity);
- `despawn()` (with its linked children, as Bevy's `ChildOf` is `linked_spawn`), `cloneAndSpawn()` (returns the new entity at once, from the pool);
- `insertRecursive(...parts)` and `removeRecursive(...classes)` (on `e` and every descendant through `Children`);
- `setParent(parent)`, `addChildren(list)`, `detachChildren()`, `despawnChildren()`, and `withChildren((parent) => ...)`, where `parent.spawn(...parts)` spawns a child of `e` and returns it.

`insert` with an element SHALL put the element's root components on `e` and its children under `e`; a fragment SHALL throw as in `spawn`. A command on an entity that no longer exists SHALL fail that command with an error naming the method and the entity, except `tryInsert`, which SHALL do nothing.

#### Scenario: insert of an element lands on the entity
- **WHEN** a system calls `commands.entity(e).insert(<entity><Health hp={5} /><entity><Name value="tip" /></entity></entity>)`
- **THEN** after the run `e` has `Health` 5 and one new child with `Name` "tip"

#### Scenario: insertIfNew
- **WHEN** `e` has `Health` 3 and a system calls `commands.entity(e).insertIfNew(Health.of({ hp: 9 }), Shield.of())`
- **THEN** after the run `e` has `Health` 3 and `Shield`

#### Scenario: retain and clear
- **WHEN** `e` has `Health`, `Shield` and `Transform`, and a system calls `entity(e).retain(Transform)`, and on another entity `f`, `entity(f).clear()`
- **THEN** after the run `e` has only `Transform` among those, and `f` exists with no components

#### Scenario: cloneAndSpawn
- **WHEN** a system calls `const c = commands.entity(e).cloneAndSpawn()` on an `e` with `Health` 4
- **THEN** after the run `c` is a new entity with `Health` 4

#### Scenario: insertRecursive
- **WHEN** `a` has child `b`, `b` has child `c`, and a system calls `commands.entity(a).insertRecursive(Frozen.of())`
- **THEN** after the run `a`, `b` and `c` each have `Frozen`

#### Scenario: withChildren
- **WHEN** a system calls `commands.entity(p).withChildren((parent) => { parent.spawn(Health.of()); parent.spawn(Health.of()); })`
- **THEN** after the run `p` has two new children with `Health`

#### Scenario: Despawn takes the children
- **WHEN** `a` has child `b` and a system calls `commands.entity(a).despawn()`
- **THEN** after the run neither `a` nor `b` exists

#### Scenario: remove
- **WHEN** `e` has `Health` and `Shield` and a system calls `commands.entity(e).remove(Shield)`
- **THEN** after the run `e` has `Health` and not `Shield`

### Requirement: Single and Populated skip the system
Before calling JS for a system with a `Single` or `Populated` param, Rust SHALL count that param's matching rows, after row filters. A `Single` that does not match exactly one row, an `Option<Single>` that matches more than one, or a `Populated` that matches none SHALL skip the system for that run: no call into JS and no error, as Bevy's param validation skips it. This SHALL share the skip path of Rust-evaluated run conditions.

#### Scenario: No player
- **WHEN** a system takes `player: Single<[Mut<Transform>], With<Player>>` and no entity has `Player`
- **THEN** the system is not called, no error is reported, and the backend records no call into JS for it

#### Scenario: One player
- **WHEN** exactly one entity has `Player` and `Transform`
- **THEN** the system is called with `player` that row, and a write through it reaches the entity's `Transform`

#### Scenario: Populated with no enemies
- **WHEN** a system takes `Populated<[Enemy]>` and no entity has `Enemy`
- **THEN** the system is not called

### Requirement: RemovedComponents
`RemovedComponents<C>` SHALL give the entities that lost `C`, by `remove` or by despawn, since this system last read it, as Bevy's `RemovedComponents` does, read from the world's removed-component messages with a cursor kept per system identity. It SHALL offer iteration, `each`, `len` and `isEmpty()`. A reload SHALL keep the cursor.

#### Scenario: Removed and despawned
- **WHEN** one system removes `Health` from `a` and despawns `b` (which has `Health`), and a later system in the same frame takes `gone: RemovedComponents<Health>`
- **THEN** it sees `a` and `b`, and on its next run sees neither

### Requirement: Lifecycle Discard and Despawn, with the value
An observer's first param MAY also be `On<Discard<C>>` (before a value of `C` is replaced or removed, Bevy 0.20's `Discard`) or `On<Despawn<C>>` (an entity with `C` is despawned), beside `On<Add<C>>`, `On<Insert<C>>` and `On<Remove<C>>`. Because a TS lifecycle observer runs deferred (see "Where observers run"), Rust SHALL copy `C`'s value at trigger time for `Remove`, `Discard` and `Despawn`, and `On` SHALL give that value's fields directly, as it gives an event's fields, beside `entity`. For one despawn, the observers SHALL run in Bevy's order: `Despawn`, `Discard`, `Remove`. A lifecycle observer on a component that is not layout tier SHALL get `entity` only.

#### Scenario: Reading the value in a Remove observer
- **WHEN** an entity with `Health` 7 is despawned and a TS observer takes `on: On<Remove<Health>>`
- **THEN** the observer runs with `on.hp === 7` and `on.entity` the despawned entity

#### Scenario: Discard on replace
- **WHEN** `e` has `Health` 2, a system inserts `Health.of({ hp: 5 })` on `e`, and an observer takes `On<Discard<Health>>`
- **THEN** the observer runs once with `hp === 2`, and `e` has `Health` 5

#### Scenario: Despawn order
- **WHEN** an entity with `Health` is despawned and observers take `On<Despawn<Health>>`, `On<Discard<Health>>` and `On<Remove<Health>>`
- **THEN** they run in that order, once each

### Requirement: Entity events bubble up ChildOf
An `EntityEvent` with `static propagate = true` triggered on `c` SHALL run its observers for `c`, then each ancestor through `ChildOf` in order, with `on.entity` the current target, until an observer calls `on.propagate(false)` or the root is passed.

#### Scenario: Bubbling stops where asked
- **WHEN** `c`'s parent is `b`, `b`'s parent is `a`, `Hit` propagates, and the observer stops propagation when `on.entity` is `b`
- **THEN** the observer runs for `c` and `b`, not `a`

### Requirement: Disabled entities
TS queries SHALL honour the world's `DefaultQueryFilters`, as a Bevy query does: an entity with `Disabled` (or any registered disabling component) SHALL NOT match unless the query names that component, so `Query<[Plain]>` skips disabled entities, `Query<[Plain], With<Disabled>>` sees only them, and `Has<Disabled>` sees both (`Disabled` has no layout, so it is not query data). Disabling components SHALL be read when the generation loads.

#### Scenario: A disabled entity
- **WHEN** four entities have `Plain` and one of them also has `Disabled`
- **THEN** `Query<[Entity, Plain]>` visits three and `Query<[Entity, Plain], With<Disabled>>` visits one

### Requirement: Sparse-set components in queries
A TS query naming a sparse-set component (a Rust component with `#[component(storage = "SparseSet")]`) as data SHALL fail the load with "`<name>` is a sparse-set component; it can be a filter but not query data", the file and line. As a filter (`With`, `Without`, `Changed`, `Added`, `Or` terms) or as `Has`, it SHALL match exactly the rows a Bevy query with the same terms matches. A query whose data, filters and disabling components are all table-stored SHALL keep the dense path: whole tables, no row list built from archetypes.

#### Scenario: A sparse-set marker as a filter
- **WHEN** four entities have `Plain`, one has `Tag` (sparse-set) and one has `Disabled`, and a TS system logs each query's `len`
- **THEN** `Query<[Entity, Plain], With<Tag>>` has 1, `Without<Tag>` has 2, and `Changed<Tag>` has 0 after the first run, as Bevy's own queries report

#### Scenario: Sparse-set data
- **WHEN** a system takes `Query<[Spark]>` where `Spark` is sparse-set
- **THEN** loading fails with "`Spark` is a sparse-set component; it can be a filter but not query data", the file and line

### Requirement: Required components on TS components
A TS component class MAY declare `static require = [B, C.of({ ... })]`, as Rust's `#[require(B, C = ...)]`. Inserting it on an entity, from any path (commands, TSX, levels, Rust), SHALL insert each required component the entity lacks, with its defaults or the given value, and SHALL leave ones already present untouched. Requirements SHALL apply transitively. The required components SHALL be present before the next system runs. A cycle SHALL fail the load naming the classes. `removeWithRequires` SHALL remove them with it.

#### Scenario: An enemy gets its health
- **WHEN** `class Enemy extends Component { static require = [Health.of({ hp: 10 })] }` and a level places `<Enemy />`
- **THEN** the entity has `Health` 10 before the first `Update` system runs

#### Scenario: Present values win
- **WHEN** a system spawns `Enemy.of(), Health.of({ hp: 3 })`
- **THEN** the entity has `Health` 3
