# ts-systems Specification

## Purpose
How TS systems are declared, scheduled and run over the ECS: Rust-style signatures read at compile time, zero-copy columns, per-row change ticks, the command buffer, and the resources, messages, observers, states and plugins around them.

## Requirements

### Requirement: Systems are exported functions whose parameter types declare what they use
A TS system SHALL be an exported function whose parameters are annotated with system param types, as a Rust Bevy system is (operator ruling 2026-10-20, docs/research/dx.md):

```ts
export function chase(
  time: Res<Time>,
  player: Query<[Transform], With<Player>>,
  enemies: Query<[Enemy, Mut<Transform>], [With<Alive>, Without<Player>]>,
) { ... }
```

The engine SHALL learn each system's declarations from those annotations through the compile step (script-runtime, "System signatures are read from syntax"), never by running the function. The engine SHALL call the function once per run with one value per parameter, in order. A system's access SHALL be the union over its params: a component is written when any query names it as `Mut<C>`, and read otherwise. A function registered without declarations SHALL throw at registration with "compiled without bevy-ts" and the function's name.

#### Scenario: Declaring a system
- **WHEN** `game.ts` exports `function bob(q: Query<[Mover, Mut<Transform>]>) { ... }` and calls `app.addSystems(Update, bob)`
- **THEN** after load, `bob` runs once per `Update` over every entity with both `Mover` and `Transform`, with `Transform` written and `Mover` read

#### Scenario: Not exported
- **WHEN** a module passes a function it does not export to `addSystems`
- **THEN** loading fails with an error naming the function, file and line

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
- `NextState<S>`, `State<S>` and `PreviousState<S>`: a state machine (ts-states).
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

### Requirement: Overlapping queries need no ParamSet
A system MAY have several queries, and two of them MAY match the same entity with one writing a component the other reads. Both SHALL see the same column memory, so a write through one is visible through the other in the same run. The dev-only read-column guard SHALL hash only columns no query of the system writes.

#### Scenario: Two views of one Transform
- **WHEN** a system has `a: Query<[Entity, Mut<Transform>], With<Enemy>>` and `b: Query<[Entity, Transform], With<Enemy>>`, and writes `translation.y` through `a`
- **THEN** reading the same entity through `b` later in the run returns the written value

### Requirement: The setup closure is the fallback for computed param types
`system(setup)` SHALL declare a system whose `setup` runs once per generation and returns the per-run body. Inside `setup`, `query(...data).with(...).without(...).changed(...).added(...)`, `res`, `resMut`, `commands` and the declaring functions of ports SHALL declare params with values and return the param objects. A declaring call outside a setup SHALL fail the load with its location. This form exists for systems whose param types the compile step cannot read (computed types such as `ReturnType<...>` or `Pick<...>`); it is otherwise equivalent.

#### Scenario: A computed param type
- **WHEN** a system's param is annotated `ReturnType<typeof makeQuery>`
- **THEN** loading fails with "ReturnType is not a system param type, an alias of one, or a param bundle", the file and line; rewritten as `system(() => { const q = query(Enemy, Mut(Transform)); return () => ... })` it loads

### Requirement: Schedules, ordering and run conditions
`app.addSystems(schedule, ...systems)` SHALL register systems in a schedule (ts-schedules lists them) or a state transition (`OnEnter(state)`, `OnExit(state)`). Each TS system in a schedule SHALL be its own Bevy system in that schedule, in system set `BevyTsSet::<Stage>`, so Rust plugins can order around a schedule's TS systems as one set; the `Update` stage SHALL run before `TransformSystems::Propagate`. Each TS system SHALL declare to Bevy exactly the access its params need: a read or a write of each component its queries name, matching the archetypes its filters allow; a read or a write of each resource it names; the access of its exported params. Two params of one TS system MAY overlap (a query and an exported param over the same column); the system SHALL declare their union and its body SHALL see one column. Within a stage, systems SHALL run in registration order, adjusted by ordering: `systems(a, b)` groups systems as Bevy's tuple does, `chain(a, b, c)` orders them, and a group's `.after(x)`, `.before(x)` and `.inSet(set)` apply to each member. That run order SHALL be given to Bevy as ordering between the stage's TS systems. `x` is a system function, a set class (`class EnemySet extends SystemSet {}`, named like a component), or a set a Rust crate exposed (`systemSet("rapier:step")`, or a port's typed `PhysicsSet.StepSimulation`). A cycle SHALL fail the load. Every TS system SHALL be followed by a sync point within its stage, whatever its params: its commands, message writes, state changes, triggers and other output (signals it set, bound fields it wrote) SHALL apply there, before the next TS system of its stage runs. A stage's first TS system SHALL run after the observers Rust triggered since the last bevy-ts stage.

`.runIf(condition)` takes a condition (ts-conditions). Every condition SHALL be a Bevy run condition with its own state. On a group it SHALL be Bevy's collective `run_if` on the group's system set: evaluated once per run of the schedule, the first time a member is about to run, and its result applied to every member in that run, wherever the members run (a stage or slots). `.distributiveRunIf(condition)` SHALL give each member its own instance, evaluated before that member, as Bevy's `distributive_run_if`. On a single system the two SHALL be the same. A condition's state (a `runOnce` that ran, a change condition's tick, a timer's time) SHALL belong to that condition's instance, and SHALL be kept by the system's or group's identity across hot reload. A skipped system gated only by Rust-evaluated conditions SHALL cost no JS call.

#### Scenario: Two systems in one stage
- **WHEN** `a` and `b` are registered in that order in `Update` and `a` writes a field `b` reads
- **THEN** `b` sees `a`'s write in the same frame

#### Scenario: A spawn is seen by the next system
- **WHEN** `a` spawns an `Enemy` with `commands.spawn` and `b`, registered after `a` in `Update`, queries `Enemy`
- **THEN** `b` sees the new enemy in the same frame

#### Scenario: A skipped system
- **WHEN** `systems(chase).runIf(inState(Game.Playing))` is registered and the state is `Menu`
- **THEN** `chase` is not called and the backend records no call into JS for it

#### Scenario: Collective and distributive
- **WHEN** `systems(a, b).runIf(runOnce())` and `systems(c, d).distributiveRunIf(runOnce())` are registered in `Update`
- **THEN** on the first frame `a`, `b`, `c` and `d` all run; on later frames none of them runs

#### Scenario: A group's condition is evaluated once
- **WHEN** `systems(a, b).runIf(resourceEquals(Score.of({ value: 0 })))` is registered and `a` sets `Score.value` to 1
- **THEN** `b` still runs in that stage run, as in Bevy, and neither runs on the next frame; with `distributiveRunIf` instead, `b` would not run

#### Scenario: A group split across a stage and a slot
- **WHEN** `systems(a, b).runIf(runOnce())` is registered in `FixedUpdate` with `b` also ordered `.after(systemSet("rapier:step"))`
- **THEN** the condition is evaluated once per run of `FixedUpdate`, and `a` and `b` both run on the first run and neither on later runs

#### Scenario: Two configs with the same condition keep separate state
- **WHEN** `systems(a).runIf(runOnce())` and `systems(b).runIf(runOnce())` are registered and `b` is gated off by `inState` on the first frame
- **THEN** `a` runs once on the first frame, and `b` still runs once on the first frame its state allows

#### Scenario: A TS system is a Bevy system
- **WHEN** `move(q: Query<[Mut<Transform>, Velocity]>)` is registered in `Update`
- **THEN** the `Update` schedule holds one Bevy system named after `move`'s identity, in `BevyTsSet::Update`, whose access writes `Transform` and reads `Velocity`

#### Scenario: A Rust system that conflicts is ordered or reported
- **WHEN** a Rust system in `Update` writes `Transform`, is not ordered against `BevyTsSet::Update`, and the schedule's ambiguity detection is on
- **THEN** Bevy reports the Rust system as ambiguous with `move`, by name

#### Scenario: A predicate condition is evaluated by a gate
- **WHEN** `systems(spawnWave).runIf(ready)` is registered in `Update`, with `ready` a TS predicate
- **THEN** an exclusive gate system runs `ready` before `spawnWave` in the same frame, and `spawnWave` runs only when it returned true

#### Scenario: Access wider than exact
- **WHEN** a TS system has a reflect-tier query term, a reflected resource or a Rust `MessageReader`
- **THEN** its access writes the whole world, so no other system runs beside it; and with `reflect` every TS system reads every resource that existed when it initialized

#### Scenario: A reload keeps a condition's state
- **WHEN** `systems(intro).runIf(runOnce())` has run, and the file is saved with a change to `intro`'s body only
- **THEN** `intro` does not run again

### Requirement: One boundary crossing per system run
Running a TS system SHALL cost a fixed number of engine calls per run, independent of the number of entities and components: one call into JS for the run. Field reads and writes inside the run SHALL be plain typed-array accesses with no call into Rust.

#### Scenario: 10,000 entities
- **WHEN** a system iterates 10,000 matching entities in 4 tables
- **THEN** the backend records exactly one call into JS for that run and no calls from JS into Rust for field access

### Requirement: Columns are zero-copy views valid for one run
For each table matched by each of the system's queries and each component in its data, the backend SHALL wrap the column's bytes (`len × size` from the column's data pointer) in an `ArrayBuffer` without copying, expose typed-array views aliasing it, and detach every buffer when `run` returns or throws. For each written component (`Mut<C>`), the backend SHALL also wrap the column's changed ticks (`len × 4` bytes) in an `ArrayBuffer` without copying, exposed as a `Uint32Array`, detached with the rest. Tables with zero rows SHALL be skipped. Entity ids SHALL be copied per run into a reused `Uint32Array` of (index, generation) pairs.

#### Scenario: Writing in place
- **WHEN** a system writes `mover.speed = 3` for a row
- **THEN** the value is in the ECS column with no copy-back step, and a Rust system reading it later that frame sees 3

### Requirement: Iteration API
A query param SHALL offer `q.each((...row) => ...)`, where each element of `row` is the entity or a reused accessor object positioned on the current entity (the hot loop: no iterator protocol), iteration with `for (const [...] of q)`, `q.single()` (exactly one match, or throws), `q.get(entity)` (the row or `undefined`), `q.contains(entity)`, `q.isEmpty()`, `q.iterMany(entities)` (the rows of the listed entities that match, in list order, skipping the rest), `q.getMany(entities)` (an array of rows, throwing on the first entity that does not match), `q.iterCombinations(k)` (every set of `k` distinct rows, once each), `q.len`, `q.tick` (the run's change tick) and `q.tables`, each with `len`, `entities`, `read(T)` giving a component's typed arrays and offsets, `write(T)` giving the same for a written component and marking the rows of that table the query matched changed when the run ends, and `split(T)` giving the arrays plus the column's `ticks` (`Uint32Array`) and leaving marking to the caller. A query whose data has `ChildOf` SHALL offer `q.iterAncestors(entity)` (parent first, up to the root), and one whose data has `Children` SHALL offer `q.iterDescendants(entity)` (breadth first), as Bevy's `Query<&ChildOf>` and `Query<&Children>` do. The frame's times SHALL come from `Res<Time>` (see "Time as a resource view").

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

### Requirement: Change detection on written rows
Each TS system run SHALL take its own change tick from `World::increment_change_tick` and pass it to JS as `q.tick`. A generated accessor's setter SHALL store that tick in the row's changed tick on every write, so a row is changed when and only when a system wrote it (a write of an equal value included, as with Bevy's `Mut`). After the run, Rust SHALL set the summary tick of every written column in every matched table and SHALL fill the ticks of the rows the query matched in every table taken through `table.write(T)`. Rows a system did not write SHALL NOT be marked changed. Columns no query writes SHALL NOT be marked changed. In dev builds, bevy-ts SHALL hash those columns before and after the run and report a system that wrote to one. Setting a row's tick SHALL cost one typed-array store and no call into Rust.

#### Scenario: A Rust system filtering Changed
- **WHEN** a TS system queries `Mut<Mover>`, writes `speed` on 3 of 10,000 matched entities, and a Rust system queries `Changed` on `Mover`'s id
- **THEN** the Rust system sees exactly those 3 entities that frame

#### Scenario: A watch stream stays quiet
- **WHEN** a TS system querying `Mut<Transform>` runs every frame but writes no row of a watched entity
- **THEN** `bevy-ts/get_components+watch` sends nothing for that entity's `Transform`

#### Scenario: Raw arrays through write
- **WHEN** a system takes `table.write(Mover)` and writes one row
- **THEN** after the run every row of that table the query matched reads as changed

#### Scenario: Raw arrays under a sparse-set filter
- **WHEN** four entities share `Plain`'s table, one also has the sparse-set `QTag`, and a system with `Query<[Mut<Plain>], With<QTag>>` takes `table.write(Plain)`
- **THEN** after the run only the tagged row reads as changed to another system's `Changed<Plain>`, not its table-mates

#### Scenario: Writing a read column in dev
- **WHEN** a system writes to a component no query of it declares `Mut`, under `bevy-ts dev`
- **THEN** a warning names the system and component, once per generation

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

### Requirement: Resources
`class R extends Resource { ...fields }` SHALL declare a resource with the same field rules, naming and layout as a component (ts-components). A resource SHALL be a component on its own resource entity, as in Bevy 0.20 (`trait Resource: Component`), so `Res<R>` and `ResMut<R>` are one-row column views through the same zero-copy path as a query. `app.insertResource(R.of({...}))` SHALL insert it at the generation's apply if absent; a reload SHALL keep its value, and a `restart` reload or a Stop (hot-reload, edit-mode) SHALL insert it again with its `insertResource` value.

A layout-tier Rust resource (native-components) SHALL be reachable the same way: `Res<R>` reads its own row in place, and `ResMut<R>` writes it in place and marks its change tick, so a Rust system's `Res<R>::is_changed()` sees the write that frame. A resource name SHALL resolve to a TS resource first, then to a layout-tier Rust resource by type path or by short name when only one type has it. `Time` is not on this path (see "Time as a resource view"). The load SHALL fail, naming the system, the param and the resource, when the name is:
- a reflect-tier Rust resource named through a layout class (`nativeResource`), with the tier's reason; through its reflected handle (`bevy-ts/bevy`, `reflectClass`) it loads, as "Reflected rows in TS systems" says;
- a Rust component that is not a resource;
- a short name more than one type shares, with their type paths;
- unknown;
- or, for `ResMut`, an immutable type.

A Rust resource of either tier named as query data (`Query<[ClearColor]>`) SHALL fail the load naming the system and saying the type is a resource, not a component, to be read with `Res<R>`. It SHALL NOT report that the type is stored in a sparse set, though Bevy 0.20 stores a resource as a sparse-set component.

When the resource is absent at run time, `Res<R>` and `ResMut<R>` SHALL fail the run with an error naming the resource, as Bevy's `Res<T>` fails validation. `Option<Res<R>>` and `Option<ResMut<R>>` SHALL be params for TS and Rust resources alike: `undefined` when the resource is absent, and the system runs. `runIf(resourceExists(R))` SHALL skip the system instead.

#### Scenario: A score
- **WHEN** a system with `score: ResMut<Score>` adds 1 to `score.value`
- **THEN** a Rust system reading `Score`'s component id sees the new value that frame

#### Scenario: A score across a save and a Stop
- **WHEN** `Score` is 5, `game.ts` is saved and reloads, then a client sets `edit` after play
- **THEN** `Score` is still 5 after the save, and back at its `insertResource` value after the Stop

#### Scenario: A Bevy resource written from TS
- **WHEN** a TS system with `ambient: ResMut<GlobalAmbientLight>` sets `ambient.brightness = 400`, and a Rust system after it reads `Res<GlobalAmbientLight>`
- **THEN** the Rust system reads 400 and `is_changed()` is true that frame, and false the next frame if TS does not write again

#### Scenario: A Bevy resource read without a write
- **WHEN** a TS system takes `scale: Res<UiScale>` and the app's `UiScale` is 2
- **THEN** it reads `scale.value === 2` (a newtype's one field is `value`) and Bevy's change tick for `UiScale` does not move

#### Scenario: A reflect-tier resource
- **WHEN** a TS system takes `Res<ClearColor>` with `ClearColor` from `bevy-ts/bevy`, and the app's `ClearColor` is red
- **THEN** it loads and reads the red colour as `value`

#### Scenario: A reflect-tier resource through a layout class
- **WHEN** a TS system takes `Res<ClearColor>` with `class ClearColor extends nativeResource("ClearColor", { v: f32 })`
- **THEN** the load fails naming the system, `ClearColor` and its reflect-tier reason

#### Scenario: A resource named as query data
- **WHEN** a TS system takes `Query<[ClearColor]>` with `ClearColor` from `bevy-ts/bevy`
- **THEN** the load fails naming the system and saying `ClearColor is a resource, not a component; take Res<ClearColor>`, with no mention of a sparse set

#### Scenario: A component named as a resource
- **WHEN** a TS system takes `Res<Transform>`
- **THEN** the load fails saying `Transform` is a component, not a resource

#### Scenario: An absent Rust resource
- **WHEN** a TS system takes `Res<UiScale>` in an app with no `UiScale`
- **THEN** the run fails with an error naming `UiScale`, and the same system with `Option<Res<UiScale>>` runs and reads `undefined`

#### Scenario: An optional TS resource
- **WHEN** a system takes `score: Option<ResMut<Score>>` and `Score` is inserted only after its first run
- **THEN** the first run reads `undefined`, and later runs write `Score` in place

### Requirement: Messages
`class M extends Message { ...fields }` and `app.addMessage(M)` SHALL declare a buffered message type, as Bevy's `Message`. Messages SHALL be stored by Rust in a double-buffered arena per type, in the type's native layout, and cleared on Bevy's two-frame rule. `MessageWriter<M>` SHALL offer `write(value)` and `next()`, an allocation-free writable view on a fresh slot; `MessageReader<M>` SHALL offer `each`, iteration and `len` over messages the system has not read yet, tracked by a cursor per system identity.

#### Scenario: Two readers
- **WHEN** a system writes 3 `Damage` messages and two later systems read `Damage`
- **THEN** each reader sees the same 3 messages once

### Requirement: Observers
`class E extends Event { ...fields }` and `class E extends EntityEvent { ...fields }` (with `static propagate = true` to bubble up `ChildOf`) SHALL declare events, with the same field rules as components. An observer SHALL be an exported function whose first param is `On<E>`, `On<Add<C>>`, `On<Insert<C>>` or `On<Remove<C>>` and whose other params are system params, registered with `app.addObserver(fn)`. `On<E>` SHALL give the event's fields directly (`explode.radius`, as Rust's `On` derefs to the event), the current target as `entity` (an entity event's own field; for a lifecycle event the entity it happened to), the first target of a bubbling event as `original` (Bevy's `original_event_target`; for a lifecycle event the entity it happened to; undefined for a global event), `propagate(on)`, and the event's whole value as `event` (Bevy's `On::event()`: its fields, and for an entity event `entity`, the current target at each hop as Bevy rewrites it when the event propagates; the first target is `original`). An entity event SHALL carry its target as its `entity` field, and `commands.trigger(Explode.of({ entity: e, radius: 3 }))` SHALL trigger it on `e`, as Rust's `commands.trigger(Explode { entity: e, radius: 3.0 })`. An observer triggered from a TS system's commands SHALL run when those commands are applied, after that system. An observer triggered from Rust (a lifecycle event of a Rust insert, a picking event) SHALL run at the next bevy-ts stage boundary in the same frame, in trigger order, with its propagation path computed in Rust at trigger time. `entity`, `original`, `propagate`, `toJSON` and `event` are `On<E>`'s own, so a TS event field by one of those names SHALL fail the load at the field with "field X shadows On<E>.X; rename it" (an `EntityEvent`'s `entity` is its target and stays allowed).

`commands.entity(e).observe(fn)`, as Bevy's `EntityCommands::observe`, SHALL attach an observer to `e` alone, for a TS or Rust entity event. `fn` SHALL be an exported function whose first param is `On<E>`; it need not be passed to `addObserver`. The observer SHALL be built on first use, keyed by the function's declaration (`file#name`), and SHALL follow that declaration across a reload. It SHALL run only for triggers whose current target is `e`, bubbling hops included, and SHALL be removed when `e` is despawned. A function whose first param is not an entity event SHALL fail that run's commands with an error naming it.

#### Scenario: Triggered from TS
- **WHEN** system `a` calls `commands.trigger(Explode.of({ entity: e, radius: 3 }))` and an observer on `On<Explode>` despawns `e`
- **THEN** the next system in the stage no longer sees `e`

#### Scenario: A field named like On's own
- **WHEN** `class Hit extends Event` declares a field `propagate`
- **THEN** the load fails at that field with "field propagate shadows On<Hit>.propagate; rename it"

#### Scenario: An observer on one entity
- **WHEN** a system calls `commands.entity(a).observe(onHit)` where `onHit` takes `On<Hit>`, and later `Hit` is triggered on `a` and on `b`
- **THEN** `onHit` runs once, for `a`; after `a` is despawned a `Hit` on a new entity does not run it

#### Scenario: An entity observer on a Rust event
- **WHEN** a system calls `commands.entity(button).observe(clicked)` where `clicked` takes `On<PointerClick>`, and the user clicks `button`
- **THEN** `clicked` runs at the next bevy-ts stage start with `entity` the button

#### Scenario: Reload keeps entity observers
- **WHEN** `clicked`'s body changes and the module reloads while `button` lives
- **THEN** the next click on `button` runs the new body, once

### Requirement: Plugins
A plugin SHALL be a function taking the app, `(app: App) => void`, registered with `app.addPlugins(fn)`, as Bevy's `Plugin::build`. The loader SHALL re-run every plugin on every generation; nothing a plugin registers outlives a reload except the ECS state it set up.

#### Scenario: A plugin's systems on reload
- **WHEN** a plugin's body changes to register one more system and the module reloads
- **THEN** the new generation has that system and the plugin's resources keep their values

### Requirement: Time as a resource view
`Res<Time>`, `Res<Time<Real>>`, `Res<Time<Virtual>>` and `Res<Time<Fixed>>` SHALL each give a read-only view of that Bevy clock, built per run from the world with no reflection, with Bevy's getter names as snake_case properties and every `Duration` as seconds (`number`). `bevy-ts` SHALL export a generic `Time<C>` and the marker types `Real`, `Virtual` and `Fixed`; `Time` alone is `Time<()>`, as in Bevy.

Every clock SHALL have `delta_secs`, `elapsed_secs` and `elapsed_secs_wrapped` (`f32`), and `delta_secs_f64`, `elapsed_secs_f64`, `elapsed_secs_wrapped_f64` and `wrap_period` (`f64`). `Time<Virtual>` SHALL add `is_paused`, `was_paused`, `relative_speed`, `relative_speed_f64`, `effective_speed`, `effective_speed_f64` and `max_delta`. `Time<Fixed>` SHALL add `timestep`, `overstep`, `overstep_fraction` and `overstep_fraction_f64`. `Time<Real>`'s `Instant`s SHALL NOT be visible.

`Res<Time>` SHALL read Bevy's generic `Time` for the schedule the system runs in: `Time<Virtual>`'s values in `Update`, `Time<Fixed>`'s in `FixedUpdate` and every fixed schedule. Any resource whose path is `bevy_time::time::Time` with or without a context (`Time<Real>`, `bevy_time::time::Time<bevy_time::fixed::Fixed>`) SHALL route to this view and to no other resource path. `ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>` SHALL write the clock ("Writing the game and fixed clocks"). `ResMut<Time>` and `ResMut<Time<Real>>` SHALL fail the load with its location, naming `ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>` as the clocks TS writes.

#### Scenario: Fixed step
- **WHEN** a `FixedUpdate` system reads `time.delta_secs` from `Res<Time>` with the timestep at 1/64 s
- **THEN** it reads 1/64 on every run, not the frame's delta

#### Scenario: The game clock's state
- **WHEN** Rust sets `Time<Virtual>`'s relative speed to 0.5 and a TS system reads `Res<Time<Virtual>>`
- **THEN** it reads `relative_speed` 0.5 and `is_paused` false, and `Res<Time<Real>>`'s `delta_secs` is about twice `Res<Time>`'s

#### Scenario: Fixed-step reads
- **WHEN** an `Update` system reads `Res<Time<Fixed>>`
- **THEN** `timestep` is Bevy's timestep in seconds and `overstep_fraction` is in `[0, 1)`

#### Scenario: Writing the real clock
- **WHEN** a system takes `ResMut<Time<Real>>`
- **THEN** loading fails with the file and line, naming `ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>` as the clocks TS writes

### Requirement: Changed and Added rows reach JS as row lists
For a query with a row filter (`Changed<C>`, `Added<C>`, `Spawned`, or an `Or` holding one), Rust SHALL evaluate the filter per matched row against the system's last-run tick, kept per system identity, and SHALL hand JS the matching row indices per table. `Changed<C>` and `Added<C>` SHALL read `C`'s ticks; `Spawned` SHALL read the entity's spawn tick. Inside an `Or`, a `With`/`Without` term SHALL hold for every row of a table or none. `each`, iteration, `get`, `single` and the query methods SHALL visit only those rows; `table.rows` SHALL expose them, and `table.read`, `write` and `split` SHALL still cover every row.

#### Scenario: Only changed rows
- **WHEN** a Rust system changes `Health` on 2 of 1,000 entities and a TS system with `Query<[Health], Changed<Health>>` runs next
- **THEN** its `each` visits exactly those 2 rows

### Requirement: Where observers run
An observer triggered by a TS system's `commands.trigger` SHALL run when that system's commands are applied, before the next system. An observer of a lifecycle event fired while a TS system's commands are applied SHALL run at the same point. An observer triggered from Rust SHALL run at the start of the next bevy-ts stage in the same frame. Each triggered event SHALL be one call into JS.

#### Scenario: A lifecycle observer on a Rust insert
- **WHEN** a Rust `Update` system inserts `Health` on an entity and a TS observer takes `On<Add<Health>>`
- **THEN** the observer runs at the start of the `PostUpdate` bevy-ts stage that frame

### Requirement: Entities hidden from TS
`bevy_ts::ScriptHidden` SHALL be a marker component that keeps an entity out of TS: no TS query SHALL match it, no TS lifecycle observer SHALL run for it, and `bevy-ts/entities` and `bevy-ts/entities+watch` SHALL leave it out. It is for editor-only entities.

#### Scenario: An editor camera
- **WHEN** a Rust plugin spawns a camera with `Transform` and `ScriptHidden`, and a TS system queries `Query<[Mut<Transform>]>`
- **THEN** the system never sees the camera, and `bevy-ts/entities` does not list it

### Requirement: Commands insert reflected values
`commands.spawn` and `entity(e).insert` SHALL take reflect-tier values (`Projection.of({ Orthographic: {} })`, or the class for its default) and layout-tier values that set holes (`DirectionalLight.of({ illuminance: 8000, color: hex("#ffd7a8") })`). A reflect value SHALL cross as JSON in its own command op and be written by `bevy_ts::reflect_value::write` over the type's `ReflectDefault` or `ReflectFromWorld`; a layout value's holes SHALL be written the same way right after its bytes, in the same command. A value that fails to write SHALL fail that command with an error naming the type and the field, and the system's other commands SHALL still apply. A spawn naming a removed built-in recipe SHALL fail with an error naming its replacement.

#### Scenario: An orthographic camera from TS
- **WHEN** a `Startup` system calls `commands.spawn(Camera3d, Transform.lookingAt([10, 10, 10], [0, 0, 0]), Projection.of({ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 12 } } } }))`
- **THEN** the camera's `Projection` is `Orthographic` with `FixedVertical { viewport_height: 12 }` and every other field at its default, and its transform looks at the origin

#### Scenario: A warm sun from TS
- **WHEN** a system spawns `DirectionalLight.of({ illuminance: 8000, color: srgb(1, 0.85, 0.7) })`
- **THEN** the light has both values

#### Scenario: A removed recipe
- **WHEN** a system calls `commands.spawnNative("camera3d", {})`
- **THEN** the command fails with an error saying the recipe was removed and naming `commands.spawn(Camera3d, Transform.lookingAt(...))`

### Requirement: Transform.lookingAt
`Transform.lookingAt(eye, target, up = [0, 1, 0])` SHALL return a `Transform` value at `eye` whose rotation matches Bevy's `Transform::looking_at(target, up)`, computed in JS with no host call.

#### Scenario: Matches Bevy
- **WHEN** a script builds `Transform.lookingAt([-3, 3, 6], [0, 0, 0])`
- **THEN** its rotation equals Bevy's `Transform::from_xyz(-3., 3., 6.).looking_at(Vec3::ZERO, Vec3::Y)` within 1e-6 per lane

### Requirement: Reflected rows in TS systems
A reflect-tier component SHALL be accepted as query data (`C`, `Mut<C>`, `Option<C>`) and a reflect-tier resource as `Res<R>`/`ResMut<R>`, and a layout-tier row SHALL offer each hole as a getter and setter. A reflected row's value SHALL be cached across runs, keyed by the component's changed tick: a row whose tick has not moved SHALL return the cached value with no host call. A miss SHALL cost one host call, filled through a binary codec compiled once per type from its `TypeInfo`, whose decoded value equals `JSON.parse` of `bevy_ts::reflect_value::read`'s JSON. Rows the system does not touch SHALL cost no host call. A write through `Mut` SHALL be applied at once with `write`'s semantics, mark the component changed, and drop the cache entry, so the next read fetches what Rust made of it. A field assigned on a `Mut` row SHALL keep the in-place changes made to that row's value earlier in the run. Reflected writes SHALL NOT invalidate column views open in the same run. Writing a component Bevy computes (`GlobalTransform`, `ComputedNode`) SHALL throw naming it.

A reflected member (a layout-tier hole; a reflect-tier row's `value` and struct fields) SHALL be typed for reading as `read` gives it, on every row kind (query data, `Mut`, `Ref`, `Option`, `Res`, `ResMut`, `ref.get`): struct fields present, a 64-bit integer a `number`, a `Color` in Bevy's shape (`ColorRead`), a `Handle<T>` its path, ref or `null`. An assignment on `Mut` or `ResMut` SHALL take every form `write` takes. TSX props and `.of()` SHALL keep the write type. `Res` and `ResMut` of a layout-tier resource SHALL have its holes.

#### Scenario: Switching a camera to orthographic
- **WHEN** a system with `Query<[Mut<Projection>], With<Camera3d>>` sets `p.value = { Orthographic: {} }`
- **THEN** the camera renders orthographic that frame, and a later system with `Changed<Projection>` sees the row

#### Scenario: An unchanged row across runs
- **WHEN** a system reads `p.value` of a camera's `Projection` on two frames and nothing changes it in between
- **THEN** the second read makes no host call and returns an equal value

#### Scenario: Untouched rows
- **WHEN** a system queries 1,000 entities with `Query<[Transform, DirectionalLight]>` and reads only `translation`
- **THEN** no reflection host call is made in that run

#### Scenario: An atlas index reads as a number
- **WHEN** a system with `Query<[Mut<Sprite>]>` writes `const i: number = sprite.texture_atlas!.index`
- **THEN** it type-checks, and `sprite.texture_atlas = { index: "3" }` type-checks too

#### Scenario: A hole reads in Bevy's shape and takes any form
- **WHEN** a system with `Query<[Mut<DirectionalLight>]>` reads `light.color` and assigns `light.color = "#ffd7a8"`
- **THEN** the read is typed `ColorRead` and the assignment type-checks; on `Query<[DirectionalLight]>` the assignment is a type error

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

#### Scenario: layout-only commands refuse a reflected value
- **WHEN** a system calls `commands.entity(e).insertIfNew(DirectionalLight.of({ illuminance: 1, color }))`, whose `color` is written through reflection
- **THEN** the call throws "insertIfNew takes layout values only", and so do `tryInsert`, `insertRecursive`, `spawnBatch` and `insertBatch`

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

### Requirement: GlobalTransform reads as Bevy's GlobalTransform
`GlobalTransform` SHALL be a class `bevy-ts` ships typed, as `Transform` is, and `bevy-ts/bevy` SHALL re-export it rather than generate a handle for it. Its rows (query data, `Ref`, `Option`, and `ref.get` in TSX) SHALL offer Bevy 0.20's `GlobalTransform` read methods by Bevy's names in camelCase, computed from its affine with Bevy's math:

- `translation()`, `rotation()`, `scale()`, `toScaleRotationTranslation()` and `computeTransform()` (a `Transform`);
- `affine()` (the 12 floats, column-major, as `Affine3A::to_cols_array`) and `toMatrix()` (16 floats, as `Mat4::to_cols_array`);
- `right()`, `left()`, `up()`, `down()`, `back()` and `forward()`, each a unit `Vec3`;
- `transformPoint(p)`, `transformVector(v)`, `reparentedTo(parent)` (a `Transform`) and `mulTransform(t)` (a `GlobalTransform`).

Each SHALL equal Bevy's method on the same value within 1e-5 per lane. A vector or quaternion a method returns SHALL be a fresh value the caller may change. The row's `value` SHALL stay the 12 affine floats, so a host function taking a transform still takes the row. The read SHALL keep the reflected row's cost: cached by changed tick, one host call on a miss, none for a row not read. A write SHALL still throw naming `GlobalTransform`; `GlobalTransform.of(...)` and the `from*` constructors are not offered.

#### Scenario: Reading a child's world position
- **WHEN** a child at local `[1, 0, 0]` has a parent at `[0, 2, 0]`, rotated a quarter turn about Y and scaled 2, and a TS system in `PostUpdate` after `bevy:transform-propagate` with `Query<[GlobalTransform]>` reads `g.translation()`, `g.rotation()`, `g.scale()`, `g.forward()` and `g.transformPoint([0, 0, 1])`
- **THEN** each equals what a Rust system reads from the same `GlobalTransform` that frame, within 1e-5 per lane

#### Scenario: After propagation
- **WHEN** an `Update` system moves an entity's `Transform` 1 along X each frame and a `PostUpdate` system after `bevy:transform-propagate` logs `g.translation().x`
- **THEN** it logs 1 on the first frame and 2 on the second

#### Scenario: A row passed to a host function
- **WHEN** a system passes a `GlobalTransform` row to a host function that takes a transform
- **THEN** the function receives that `GlobalTransform` as given, as before this change

#### Scenario: A write
- **WHEN** a system takes `Mut<GlobalTransform>`
- **THEN** tsc rejects it, and a write that reaches the runtime throws naming `GlobalTransform`

### Requirement: Delayed commands
Every TS `Commands` (a system's, an observer's, a one-shot system's, and a delayed one) SHALL offer `delayed()`, Bevy 0.20's `DelayedCommandsExt::delayed`, returning a `DelayedCommands` with `secs(s)` and `duration(s)`. Each SHALL return a `Commands` whose commands run `s` seconds later; `secs` SHALL take its delay as Bevy's `Duration::from_secs_f32` of `s` rounded to `f32`, `duration` as `Duration::from_secs_f64(s)`. Every method of `Commands` and `EntityCommands` SHALL work on a delayed `Commands`, in call order within its queue, with the same semantics as when not delayed. A delay that is negative, `NaN`, infinite or past `Duration`'s range SHALL throw at the call, naming the method and the value, and queue nothing.

Calls on one `DelayedCommands` whose delays make the same `Duration` SHALL share one queue, as Bevy's map keyed by `Duration` does. Each queue SHALL become, when the run's commands apply, one Bevy `DelayedCommandQueue` entity with `submit_at` the default clock's (`Time`) elapsed time plus the delay, so a queue made in `FixedUpdate` counts from `Time<Fixed>`'s elapsed time, as in Bevy. Bevy's `check_delayed_command_queues` SHALL fire it in `PreUpdate` once `Time`'s elapsed time reaches `submit_at`: its commands apply then, as one TS run's commands do (all or nothing, warnings to the script log). So a delay SHALL follow the game clock: it waits while `Time<Virtual>` is paused, by the game or the editor, and runs at the clock's relative speed. Queues that come due on the same frame SHALL keep no order between them, as in Bevy.

`spawn`, `spawnBatch`, `cloneAndSpawn` and `withChildren`'s `spawn` on a delayed `Commands` SHALL return the entity at once, as Bevy allocates it at once; commands queued later, delayed or not, MAY name it. A run that throws SHALL drop its delayed commands with the rest. A `Commands` or `DelayedCommands` kept past its run SHALL throw on use, naming `delayed`.

A queue SHALL belong to the generation that made it. A restart (a Stop, a restarting reload) SHALL despawn every TS queue with no drop warning. A queue that comes due under a later generation SHALL apply nothing and log one warning naming the generation that made it and how many commands it held. `DelayedCommandQueue` SHALL read in TS as the reflected component it is (`submit_at` in seconds, the queue not visible) and SHALL NOT be inserted from TS.

#### Scenario: Bevy's own test, from TS
- **WHEN** with frames of 0.2 s, a TS `Startup` system calls `commands.delayed().secs(0.1).spawn(Dummy)`, `commands.spawn(Dummy)`, then on one `const d = commands.delayed()` calls `d.secs(0.5).spawn(Dummy)` and three `d.duration(1).spawn(Dummy)`
- **THEN** over ten frames the count of `Dummy` entities is, frame by frame, what Bevy's `delayed_queues_should_run_with_time_plugin_enabled` asserts for the same calls in Rust, and three `DelayedCommandQueue` entities exist after the first frame

#### Scenario: Spawn now, despawn later
- **WHEN** a system calls `const e = d.secs(1).spawn(Dummy)` then `d.secs(2).entity(e).despawn()`
- **THEN** `e` does not exist before 1 s of game time, exists from 1 s, and is gone from 2 s

#### Scenario: Paused game clock
- **WHEN** a system queues `commands.delayed().secs(1).spawn(Dummy)` and the game then pauses `Time<Virtual>` for 3 s of wall time
- **THEN** no `Dummy` exists until 1 s of game time has passed after the unpause

#### Scenario: Nested delays add up
- **WHEN** a system calls `commands.delayed().secs(1).delayed().secs(1).spawn(Dummy)`
- **THEN** the `Dummy` spawns once 2 s of game time have passed, not 1 s

#### Scenario: A bad delay
- **WHEN** a system calls `commands.delayed().secs(-1)`
- **THEN** the call throws naming `secs` and `-1`, and the run's other commands apply if the system catches it

#### Scenario: A Stop drops pending queues
- **WHEN** a system queues `commands.delayed().secs(5).spawn(Dummy)`, the client stops, then plays again, and 10 s pass
- **THEN** that `Dummy` never spawns, no `DelayedCommandQueue` from the first run exists, and no drop warning is logged

#### Scenario: A reload drops pending queues
- **WHEN** a system queues `commands.delayed().secs(5).spawn(Dummy)` and a reload applies before 5 s pass
- **THEN** at 5 s nothing spawns and the script log holds one warning naming the old generation and one command

### Requirement: Message writers and readers carry heap fields
A message with a heap field SHALL be written by `MessageWriter.write(M.of({...}))`, `commands.write` and `MessageWriter.next()`, whose view's heap fields are assigned whole (`m.text = "hi"`) and sent with the record; a heap field left unset SHALL take its default. A reader SHALL read a message's heap field on demand, at most one host call per field per message for the record's life. A message type with no heap field SHALL keep today's queue and make no host call.

#### Scenario: Writing through next
- **WHEN** a system calls `const m = writer.next(); m.text = "go"; m.amount = 2` and a later reader reads `Chat`
- **THEN** it reads `text` `"go"` and `amount` 2

#### Scenario: A plain message is unchanged
- **WHEN** a system reads 1,000 `Damage` messages whose fields are all plain
- **THEN** it makes no host call for them

### Requirement: The editor clock leaves a game's pause alone
bevy-ts's editor clock (`bevy-ts/pause`, `bevy-ts/step`, edit mode, a Stop) SHALL record whether it paused `Time<Virtual>` itself, and SHALL unpause `Time<Virtual>` only to undo its own pause. A pause of `Time<Virtual>` by the game, from Rust or TS, SHALL hold until the game unpauses it. A frame the editor steps while the game has paused `Time<Virtual>` SHALL run the bevy-ts stages and advance zero virtual time.

#### Scenario: A game pause survives the next frame
- **WHEN** a Rust system calls `Time<Virtual>::pause()` in a running game
- **THEN** `Time<Virtual>` is still paused on the next 10 frames, and `Res<Time>`'s `delta_secs` is 0 on each

#### Scenario: Editor pause over a game pause
- **WHEN** the game has paused `Time<Virtual>`, the client pauses with `bevy-ts/pause`, then resumes
- **THEN** the bevy-ts stages run again and `Time<Virtual>` stays paused

### Requirement: Writing the game and fixed clocks
`ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>` SHALL write Bevy's clock through exports bevy-ts registers in every engine, `VirtualTime` and `FixedTime` in `bevy-ts/time`, claiming those spellings (native-functions, "An export may stand for a Bevy spelling"). Each SHALL have every Bevy setter of its clock in camelCase, every `Duration` as seconds: on `Time<Virtual>` `pause()`, `unpause()`, `toggle()`, `setRelativeSpeed(r)`, `setRelativeSpeedF64(r)`, `setMaxDelta(s)` and `setWrapPeriod(s)`; on `Time<Fixed>` `setTimestep(s)`, `setTimestepSeconds(s)`, `setTimestepHz(hz)`, `discardOverstep(s)` and `setWrapPeriod(s)`. Each SHALL have the clock's getters of "Time as a resource view" as read-only properties under the same names, read live from the resource.

A write SHALL land in the resource when the call returns, at the TS system's tick, and take effect where Bevy reads it: a pause or speed in the next frame's delta, a timestep on the next fixed step. An argument Bevy would panic on (a zero max delta, timestep or wrap period, a negative or non-finite speed or Hz, a negative or non-finite number of seconds) SHALL throw in JS naming the export and the method, with Bevy's message, and leave the clock unchanged. A TS pause SHALL be a game pause under "The editor clock leaves a game's pause alone".

#### Scenario: Pausing the game clock from TS
- **WHEN** a TS system takes `time: ResMut<Time<Virtual>>` and calls `time.pause()`
- **THEN** `time.is_paused` is `true` in the same run, and from the next frame `Res<Time>`'s `delta_secs` is 0 in `Update`

#### Scenario: Slow motion
- **WHEN** a TS system calls `time.setRelativeSpeed(0.5)` on `ResMut<Time<Virtual>>` with frames of 0.1 s
- **THEN** from the next frame `Res<Time>`'s `delta_secs` is 0.05, as for the same call from Rust

#### Scenario: A faster fixed step
- **WHEN** a TS system calls `time.setTimestepHz(128)` on `ResMut<Time<Fixed>>`
- **THEN** `time.timestep` is 1/128 in the same run, and `FixedUpdate` runs twice as often as at 64 Hz from the next frame

#### Scenario: A zero timestep
- **WHEN** a TS system calls `time.setTimestep(0)`
- **THEN** the call throws naming `FixedTime` and `setTimestep`, the timestep is unchanged and nothing panics

#### Scenario: A TS pause over an editor step
- **WHEN** the editor has paused, a client steps one frame, and in it a TS system calls `time.pause()`, then the client resumes
- **THEN** `Time<Virtual>` stays paused
