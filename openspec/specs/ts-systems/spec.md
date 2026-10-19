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

- `Query<[data...], filter>`: data is a component class (read), `Mut<C>` (write), `Option<C>` or `Option<Mut<C>>` (the row's value or `undefined`) and `Entity`; the filter is `With<C>`, `Without<C>`, `Changed<C>`, `Added<C>` or a tuple of them. `Changed<C>` and `Added<C>` match rows whose tick is newer than this system's last run.
- `Res<R>` and `ResMut<R>`: a resource, read-only or writable.
- `Local<{ field: type, ... }>`: per-system state with a layout (field types `f32`, `u32`, ... as in ts-components), kept in Rust under the system's identity, so it survives reloads (hot-reload).
- `Commands`: the system's command buffer.
- `MessageReader<M>` and `MessageWriter<M>`: a message type's queue (see "Messages").
- `NextState<S>` and `State<S>`: a TS state machine (see "States").
- Param kinds that native ports register (for example `ReadRapierContext`), declared in their types package and known to the compile step.

A param type that is none of these, an alias of one, or an object type whose properties are these (a param bundle, Bevy's `#[derive(SystemParam)]`) SHALL fail the load with its file and line.

#### Scenario: A param bundle
- **WHEN** a system takes `{ score, time }: Scoring` where `interface Scoring { score: ResMut<Score>; time: Res<Time> }`
- **THEN** it receives an object with a writable `score` and a read-only `time`

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
`app.addSystems(schedule, ...systems)` SHALL register systems. `Startup`, `PreUpdate`, `Update`, `PostUpdate` and `FixedUpdate` SHALL map onto the bevy-ts stages; `OnEnter(state)` and `OnExit(state)` onto state transitions (see "States"). Each stage SHALL be one exclusive system per schedule, in system set `BevyTsSet::<Stage>`, so Rust plugins can order around it; the `Update` stage SHALL run before `TransformSystems::Propagate`. Within a stage, systems SHALL run in registration order, adjusted by ordering: `systems(a, b)` groups systems as Bevy's tuple does, `chain(a, b, c)` orders them, and a group's `.after(x)`, `.before(x)`, `.inSet(set)` and `.runIf(condition)` apply to each member. `x` is a system function or a set from `systemSet(name)`. A cycle SHALL fail the load. `inState`, `resourceExists`, `onMessage` and `not` SHALL be evaluated in Rust, so a skipped system costs no JS call. A system in `Startup` SHALL run once per identity per world (hot-reload).

#### Scenario: Two systems in one stage
- **WHEN** `a` and `b` are registered in that order in `Update` and `a` writes a field `b` reads
- **THEN** `b` sees `a`'s write in the same frame

#### Scenario: A skipped system
- **WHEN** `systems(chase).runIf(inState(Game.Playing))` is registered and the state is `Menu`
- **THEN** `chase` is not called and the backend records no call into JS for it

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
A query param SHALL offer `q.each((...row) => ...)`, where each element of `row` is the entity or a reused accessor object positioned on the current entity (the hot loop: no iterator protocol), iteration with `for (const [...] of q)`, `q.single()` (exactly one match, or throws), `q.get(entity)` (the row or `undefined`), `q.len`, `q.tick` (the run's change tick) and `q.tables`, each with `len`, `entities`, `read(T)` giving a component's typed arrays and offsets, `write(T)` giving the same for a written component and marking every row of that table changed when the run ends, and `split(T)` giving the arrays plus the column's `ticks` (`Uint32Array`) and leaving marking to the caller. The frame's `dt`, `elapsed` and frame number SHALL come from `Res<Time>`.

#### Scenario: each and tables agree
- **WHEN** the same system body is written once with `q.each` and once with `q.tables`
- **THEN** both produce identical component values after one frame

#### Scenario: split marks only what the caller marks
- **WHEN** a system takes `split(Mover)` on a table, writes rows 3 and 7 and sets `ticks[3] = q.tick` and `ticks[7] = q.tick`
- **THEN** after the run only rows 3 and 7 of that table read as changed

### Requirement: Change detection on written rows
Each TS system run SHALL take its own change tick from `World::increment_change_tick` and pass it to JS as `q.tick`. A generated accessor's setter SHALL store that tick in the row's changed tick on every write, so a row is changed when and only when a system wrote it (a write of an equal value included, as with Bevy's `Mut`). After the run, Rust SHALL set the summary tick of every written column in every matched table and SHALL fill the ticks of every table taken through `table.write(T)`. Rows a system did not write SHALL NOT be marked changed. Columns no query writes SHALL NOT be marked changed. In dev builds, bevy-ts SHALL hash those columns before and after the run and report a system that wrote to one. Setting a row's tick SHALL cost one typed-array store and no call into Rust.

#### Scenario: A Rust system filtering Changed
- **WHEN** a TS system queries `Mut<Mover>`, writes `speed` on 3 of 10,000 matched entities, and a Rust system queries `Changed` on `Mover`'s id
- **THEN** the Rust system sees exactly those 3 entities that frame

#### Scenario: A watch stream stays quiet
- **WHEN** a TS system querying `Mut<Transform>` runs every frame but writes no row of a watched entity
- **THEN** `bevy-ts/get_components+watch` sends nothing for that entity's `Transform`

#### Scenario: Raw arrays through write
- **WHEN** a system takes `table.write(Mover)` and writes one row
- **THEN** after the run every row of that table reads as changed

#### Scenario: Writing a read column in dev
- **WHEN** a system writes to a component no query of it declares `Mut`, under `bevy-ts dev`
- **THEN** a warning names the system and component, once per generation

### Requirement: Structural changes go through the command buffer
The `Commands` param SHALL offer `spawn(...parts)` (component values such as `Health.of({ hp: 3 })`, or a prefab element), `entity(e)` with `insert(...parts)`, `remove(...classes)`, `setParent(parent)` and `despawn()`, `trigger(event)` and `trigger(target, entityEvent)` (see "Observers"), `write(message)` and `spawnNative(name, params)`. Commands SHALL be encoded by JS into a binary arena owned by Rust, with component values in their native layout, and applied by Rust after the system returns, before the next system runs. `spawn` SHALL return a real `Entity` at once, taken from a pool allocated with `EntityAllocator::alloc`.

#### Scenario: Spawning and using the id in the same run
- **WHEN** a system spawns an entity and then calls `commands.entity(child).setParent(spawned)` in the same run
- **THEN** after the run both entities exist and the child's parent is the spawned entity

#### Scenario: The arena overflows
- **WHEN** a run queues more command bytes than the arena holds
- **THEN** the overflow is kept on the JS side and applied after the run, with no command lost and no allocation in Rust during the run

### Requirement: Native spawn recipes
Rust SHALL register named recipes callable from `spawnNative` for things TS cannot yet build: in this change `camera3d`, `directionalLight` and `cuboid` (a mesh plus a `StandardMaterial` with a colour). Their params cross as JSON; this path is for rare spawns, not per-frame work.

#### Scenario: A cube from TS
- **WHEN** a `Startup` system calls `commands.spawnNative("cuboid", { size: [1, 1, 1], color: "#f80" })` and then inserts `Mover` on the returned entity
- **THEN** an orange cube is visible and carries `game::Mover`

### Requirement: Resources
`class R extends Resource { ...fields }` SHALL declare a resource with the same field rules, naming and layout as a component (ts-components). A resource SHALL be a component on its own resource entity, as in Bevy 0.20 (`trait Resource: Component`), so `Res<R>` and `ResMut<R>` are one-row column views through the same zero-copy path as a query. `app.insertResource(R.of({...}))` SHALL insert it at the generation's apply if absent; a reload SHALL keep its value. A Rust resource with a native layout SHALL be reachable the same way (`Res<Time>`).

#### Scenario: A score
- **WHEN** a system with `score: ResMut<Score>` adds 1 to `score.value`
- **THEN** a Rust system reading `Score`'s component id sees the new value that frame

### Requirement: Messages
`class M extends Message { ...fields }` and `app.addMessage(M)` SHALL declare a buffered message type, as Bevy's `Message`. Messages SHALL be stored by Rust in a double-buffered arena per type, in the type's native layout, and cleared on Bevy's two-frame rule. `MessageWriter<M>` SHALL offer `write(value)` and `next()`, an allocation-free writable view on a fresh slot; `MessageReader<M>` SHALL offer `each`, iteration and `len` over messages the system has not read yet, tracked by a cursor per system identity.

#### Scenario: Two readers
- **WHEN** a system writes 3 `Damage` messages and two later systems read `Damage`
- **THEN** each reader sees the same 3 messages once

### Requirement: Observers
`class E extends Event { ...fields }` and `class E extends EntityEvent { ...fields }` (with `static propagate = true` to bubble up `ChildOf`) SHALL declare events, with the same field rules as components. An observer SHALL be an exported function whose first param is `On<E>`, `On<Add<C>>`, `On<Insert<C>>` or `On<Remove<C>>` and whose other params are system params, registered with `app.addObserver(fn)`. `On<E>` SHALL give `event` (a read view of the event), `entity` (the target, for entity and lifecycle events) and `propagate(on)`. An observer triggered from a TS system's commands SHALL run when those commands are applied, after that system. An observer triggered from Rust (a lifecycle event of a Rust insert, a picking event) SHALL run at the next bevy-ts stage boundary in the same frame, in trigger order, with its propagation path computed in Rust at trigger time.

#### Scenario: Triggered from TS
- **WHEN** system `a` calls `commands.trigger(e, Explode.of({ radius: 3 }))` and an observer on `On<Explode>` despawns `e`
- **THEN** the next system in the stage no longer sees `e`

### Requirement: States
`class S extends States("Value", ...) {}` SHALL declare a state machine whose values are `S.Value`, named like a component (`module#Class`, or `static path`). `app.initState(S.Initial)` SHALL create it. `NextState<S>.set(S.Value)` SHALL queue a transition applied in Bevy's `StateTransition` schedule by one bevy-ts exclusive system, which SHALL run `OnExit(old)` systems, then `OnEnter(new)` systems. `inState(S.Value)` SHALL be a Rust-evaluated run condition.

#### Scenario: Entering Playing
- **WHEN** a system calls `next.set(Game.Playing)` from `Menu`
- **THEN** in the next `StateTransition`, `OnExit(Game.Menu)` systems run, then `OnEnter(Game.Playing)` systems, and systems with `runIf(inState(Game.Playing))` run from that frame on

### Requirement: Plugins
A plugin SHALL be a function taking the app, `(app: App) => void`, registered with `app.addPlugins(fn)`, as Bevy's `Plugin::build`. The loader SHALL re-run every plugin on every generation; nothing a plugin registers outlives a reload except the ECS state it set up.

#### Scenario: A plugin's systems on reload
- **WHEN** a plugin's body changes to register one more system and the module reloads
- **THEN** the new generation has that system and the plugin's resources keep their values
