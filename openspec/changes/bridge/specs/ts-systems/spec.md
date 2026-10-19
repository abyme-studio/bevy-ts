## ADDED Requirements

### Requirement: Systems are declared with stage, access and a run function
The `bevy-ts` module SHALL export `app.system(name, { stage, read, write, with?, without?, after?, run })`. `stage` is one of `preUpdate`, `update`, `postUpdate`, `fixedUpdate`. `read` and `write` list components whose data the system sees; `with`/`without` filter without access. `app.startup(name, run)` SHALL declare a function run once per world (see hot-reload). A component in both `read` and `write` SHALL fail the load.

#### Scenario: Declaring a system
- **WHEN** `game.ts` calls `app.system("bob", { stage: "update", read: [Mover], write: [Transform], run })`
- **THEN** after load, `bob` runs once per `Update` over every entity with both `Mover` and `Transform`

### Requirement: Stage placement and ordering
Each stage SHALL be one exclusive system per schedule (`PreUpdate`, `Update`, `PostUpdate`, `FixedUpdate`), in system set `BevyTsSet::<Stage>`, so Rust plugins can order around it. Within a stage, systems SHALL run in declaration order, adjusted by `after`. A cycle in `after` SHALL fail the load. The `update` stage SHALL run before `TransformSystems::Propagate`.

#### Scenario: Two systems in one stage
- **WHEN** `a` and `b` are declared in that order in `update` and `a` writes a field `b` reads
- **THEN** `b` sees `a`'s write in the same frame

### Requirement: One boundary crossing per system run
Running a TS system SHALL cost a fixed number of engine calls per run, independent of the number of entities and components: one call into JS for the run. Field reads and writes inside the run SHALL be plain typed-array accesses with no call into Rust.

#### Scenario: 10,000 entities
- **WHEN** a system iterates 10,000 matching entities in 4 tables
- **THEN** the backend records exactly one call into JS for that run and no calls from JS into Rust for field access

### Requirement: Columns are zero-copy views valid for one run
For each table matched by the system's query and each component in `read`/`write`, the backend SHALL wrap the column's bytes (`len × size` from the column's data pointer) in an `ArrayBuffer` without copying, expose typed-array views aliasing it, and detach every buffer when `run` returns or throws. Tables with zero rows SHALL be skipped. Entity ids SHALL be copied per run into a reused `Uint32Array` of (index, generation) pairs.

#### Scenario: Writing in place
- **WHEN** a system writes `mover.speed = 3` for a row
- **THEN** the value is in the ECS column with no copy-back step, and a Rust system reading it later that frame sees 3

### Requirement: Iteration API
`run(q, ctx)` SHALL receive a query `q` with `q.each((entity, ...rows) => ...)`, where each row is a reused accessor object positioned on the current entity, and `q.tables`, each with `len`, `entities` and per-component typed arrays and offsets for hand-written loops. `ctx` SHALL carry `dt`, `elapsed`, `commands` and `frame`.

#### Scenario: each and tables agree
- **WHEN** the same system body is written once with `q.each` and once with `q.tables`
- **THEN** both produce identical component values after one frame

### Requirement: Change detection on written columns
After a run, every row of every table column in the system's `write` list SHALL have its changed tick set to the stage's current tick, and the column's summary tick updated. Columns in `read` SHALL NOT be marked changed. In dev builds, bevy-ts SHALL hash `read` columns before and after the run and report a system that wrote to one.

#### Scenario: A Rust system filtering Changed
- **WHEN** a TS system has `Mover` in `write` and a Rust system queries `Changed` on `Mover`'s id
- **THEN** the Rust system sees every `Mover` entity the TS system's query matched that frame

#### Scenario: Writing a read column in dev
- **WHEN** a system writes to a component it declared in `read`, under `bevy-ts dev`
- **THEN** a warning names the system and component, once per generation

### Requirement: Structural changes go through the command buffer
`ctx.commands` SHALL offer `spawn(...components)`, `despawn(entity)`, `insert(entity, ...components)`, `remove(entity, ...types)`, `setParent(entity, parent)` and `spawnNative(name, params)`. Commands SHALL be encoded by JS into a binary arena owned by Rust, with component values in their native layout, and applied by Rust after the system returns, before the next system runs. `spawn` SHALL return a real `Entity` at once, taken from a pool allocated with `EntityAllocator::alloc`.

#### Scenario: Spawning and using the id in the same run
- **WHEN** a system spawns an entity and then calls `setParent(child, spawned)` in the same run
- **THEN** after the run both entities exist and the child's parent is the spawned entity

#### Scenario: The arena overflows
- **WHEN** a run queues more command bytes than the arena holds
- **THEN** the overflow is kept on the JS side and applied after the run, with no command lost and no allocation in Rust during the run

### Requirement: Native spawn recipes
Rust SHALL register named recipes callable from `spawnNative` for things TS cannot yet build: in this change `camera3d`, `directionalLight` and `cuboid` (a mesh plus a `StandardMaterial` with a colour). Their params cross as JSON; this path is for rare spawns, not per-frame work.

#### Scenario: A cube from TS
- **WHEN** a startup calls `commands.spawnNative("cuboid", { size: [1, 1, 1], color: "#f80" })` and then inserts `Mover` on the returned entity
- **THEN** an orange cube is visible and carries `game::Mover`
