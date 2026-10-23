# native-functions Specification

## Purpose
How TS calls Rust: Bevy system params exported with `#[export]`, read or write, whose methods are synchronous host calls inside a TS system's run at its own ticks; deferred state applies after the run.

## Requirements

### Requirement: Rust exports system params to TS
A Rust crate (the project's own or a port) SHALL be able to export a Bevy `SystemParam` to TS by putting `#[bevy_ts::export(ts = "Name", module = "bevy-ts/<module>")]` on an `impl` block of the param and calling `app.ts_param::<P>()`. Every `pub fn` in that block SHALL become a method of the TS type `Name`. The macro SHALL generate the host function per method, the argument and result codecs, the TS declaration and the JS glue. Method signatures SHALL be restricted to `&self` or `&mut self` plus scalars, `bool`, `Vec2`, `Vec3`, `Quat`, `Entity`, `NativeLayout` records, `&[T]` and `&mut [T]` of those, and `Option` of those; any other type SHALL be a compile error naming the parameter. The param MAY write: `ResMut`, `Query<&mut T>` and the other shapes the coverage table supports. A type that is not a `SystemParam`, such as `&mut World`, SHALL fail to compile.

#### Scenario: Rapier's raycast
- **WHEN** the Rapier port exports `impl ReadRapierContext { pub fn cast_ray(&self, origin: Vec3, dir: Vec3, max_toi: f32, solid: bool, f: RayFilter) -> Option<RayHit> }`
- **THEN** a TS system taking `rapier: ReadRapierContext` can call `rapier.castRay(origin, dir, 1.1, true, { excludeRigidBody: e })`, typed from the Rust signature

#### Scenario: A mutable param
- **WHEN** a crate exports `impl NavEdit { pub fn block(&mut self, cell: u32) }` on a param holding `ResMut<NavMesh>`
- **THEN** the crate compiles, and a TS system calling `nav.block(7)` finds cell 7 blocked in `NavMesh` when the call returns

### Requirement: Exported params are system param kinds
An exported param's TS type SHALL be a system param kind the compile step reads from a system's signature (script-runtime), declared by the module that exports it. A TS system SHALL reach Rust behaviour only through such a param. A name that no loaded crate exports SHALL be a type error and a load error naming it.

#### Scenario: A misspelt param type
- **WHEN** a system takes `rapier: ReadRapierContxt`
- **THEN** `tsc` reports the type missing, and loading fails naming `ReadRapierContxt`, the file and line

### Requirement: Method calls are synchronous
A method call SHALL run synchronously inside the TS system's run and return its result in the same run, as one call from JS into Rust; a batched method (slices in, slices out) SHALL be one call for the whole batch. Record and slice arguments SHALL be read in place from JS memory, and record results SHALL be written into a Rust-owned arena viewed by JS without a copy, valid until the run ends. Rust SHALL fetch the param with `SystemParam::get_param` on the run's world, so a method sees the system's writes made so far in the run, as an overlapping query does (ts-systems), and a write it makes in place is seen by the system's next read of that column. A method SHALL NOT make structural changes in place: anything that spawns, inserts, removes or despawns SHALL go through a deferred param. A method SHALL NOT call back into JS. A param that fails validation SHALL throw a JS exception naming the param and method; a panic in a method SHALL be caught and thrown as an exception naming the method.

#### Scenario: 1,000 rays
- **WHEN** a system calls `rapier.castRays(rays, true, filter, hits)` with 1,000 rays
- **THEN** the backend records one call from JS into Rust, and the system reads 1,000 results before returning

#### Scenario: Before the physics context exists
- **WHEN** a system calls `rapier.castRay(...)` on a frame where Rapier's context entity has not been spawned
- **THEN** the call throws an exception naming `ReadRapierContext` and `castRay`, and the system's commands from that run are discarded

#### Scenario: A native write the system then reads
- **WHEN** a TS system holding `q: Query<[Mut<Health>]>` calls a method whose param holds `Query<&mut Health>` and sets entity `e`'s health to 5, then reads `e`'s health through `q`
- **THEN** it reads 5

### Requirement: The engine exports Cameras
Every engine (`bevy-ts dev`, `bevy-ts play`, a shipped game, `bevy-ts types`) SHALL export a read-only param `Cameras` from module `bevy-ts/camera`, over Bevy's `Query<(&Camera, &GlobalTransform)>`, through the exported-param mechanism. It SHALL have these methods, all in logical pixels with the origin at the viewport's top-left, the units of a UI node's `left` and `top` at the default `UiScale`:

- `worldToViewport(camera: Entity, point: Vec3): Vec2 | undefined`, Bevy's `Camera::world_to_viewport`. It SHALL return `undefined` for every viewport conversion error (no viewport size, past the near plane, past the far plane, invalid data). It SHALL NOT bounds-check: a point in front of the camera but off screen SHALL return coordinates outside `[0, size]`.
- `logicalViewportSize(camera: Entity): Vec2 | undefined`, `undefined` while the viewport size is unknown.
- `worldsToViewport(camera: Entity, points: Float32Array, out: Float32Array): number`, one call for many points: `points` holds three floats per point, `out` receives two per point (`NaN, NaN` for one that does not project), and it returns how many projected. It SHALL throw when `points` is not a multiple of three floats or `out` holds fewer than two per point.

An `Entity` that is not a camera SHALL throw, naming `Cameras`, the method and the entity. A method SHALL read the `GlobalTransform` as last propagated, so from `Update` it reflects the previous frame's transforms.

#### Scenario: A point on screen
- **WHEN** a headless 960x540 engine has a camera at `[0, 0, 10]` looking at the origin, and a system calls `cameras.worldToViewport(cam, [0, 0, 0])`
- **THEN** it returns `{ x: 480, y: 270 }`, within a pixel

#### Scenario: Off screen in front
- **WHEN** the system projects a point in front of the camera but far to its right
- **THEN** it returns an `x` greater than 960, not `undefined`

#### Scenario: Behind the camera
- **WHEN** the system projects `[0, 0, 20]`, behind the camera
- **THEN** it returns `undefined`

#### Scenario: Before the viewport has a size
- **WHEN** a system projects a point on the frame the camera is spawned, before Bevy has computed its viewport
- **THEN** `worldToViewport` and `logicalViewportSize` return `undefined`, and a later frame returns values

#### Scenario: Many points in one call
- **WHEN** a system calls `cameras.worldsToViewport(cam, points, out)` with one point on screen and one behind
- **THEN** the backend records one host call, it returns `1`, `out` holds the first point's coordinates then `NaN, NaN`

#### Scenario: Not a camera
- **WHEN** a system calls `cameras.worldToViewport(e, p)` with an entity that has no `Camera`
- **THEN** the call throws an exception naming `Cameras`, `worldToViewport` and the entity

### Requirement: Calls use the TS system's ticks
Every exported param call, read or write, SHALL be fetched at the calling TS system's own `(last_run, tick)`, and SHALL NOT advance the world's change tick. A write SHALL carry the run's tick, as a TS `Mut` write of the same system does. A read's `is_changed`, `Ref::is_changed` and `Changed<T>` SHALL mean "since this TS system last ran". A call from a TS entry (`call_entry`) SHALL use the entry's tick and the world's last change tick.

#### Scenario: A system does not see its own native write
- **WHEN** TS system `a` writes `Score` through an exported param in frame 1, and nothing else writes it
- **THEN** in frame 2 the same param's `Res<Score>::is_changed()` called from `a` is false, and from a TS system that last ran before frame 1's write it is true

#### Scenario: Calls spend no ticks
- **WHEN** a TS system makes 1,000 exported param calls in one run
- **THEN** the world's change tick after the run is what it would be with no calls

#### Scenario: Rust sees the write this frame
- **WHEN** a TS system writes `Health` through an exported param, and a Rust system with `Query<&Health, Changed<Health>>` runs after it in the same frame
- **THEN** the Rust system sees the entity

### Requirement: Deferred params apply after the run
An exported param's deferred state (`Commands`, `Gizmos`, any `Deferred<T>`) SHALL apply once per run, after the TS system's own output and before observers drain, one param after another in the system's parameter order. A run that throws SHALL drop every exported param's deferred state from that run, as it drops the TS system's commands.

#### Scenario: A command from Rust
- **WHEN** a method on a param holding `Commands` spawns an entity during a TS system's run
- **THEN** the entity does not exist during the run and exists when the next system runs

#### Scenario: A throw after a queued command
- **WHEN** a method queues a spawn, and the TS system then throws
- **THEN** no entity is spawned

### Requirement: Params with unbounded access are refused
`app.ts_param::<P>()` SHALL refuse a param whose access writes every component or every resource (`Query<EntityMut>`, `EntityMutExcept`, `DeferredWorld`), naming the param and saying it must name the components and resources it writes. `FilteredEntityMut` and `FilteredResourcesMut` register no access without a builder, which an export never has, so they SHALL reach nothing: every read or write through them fails.

#### Scenario: EntityMut
- **WHEN** a crate calls `app.ts_param::<P>()` where `P` holds `Query<EntityMut>`
- **THEN** the app fails to build with an error naming `P` and `EntityMut`

### Requirement: Slices must not alias the run's columns
A slice argument SHALL be rejected, with an exception naming the param, method and argument, when its bytes overlap a column or resource row the run exposes to JS and either the slice is `&mut` or the param may write that column or resource.

#### Scenario: A column passed to a writer
- **WHEN** a TS system passes `Health`'s column array from `q.tables` as a `&[f32]` argument to a method whose param holds `Query<&mut Health>`
- **THEN** the call throws naming the param, method and argument, and the method does not run

#### Scenario: A column passed to a reader
- **WHEN** the same column is passed as `&[f32]` to a method whose param holds `Query<&Health>`
- **THEN** the call runs

### Requirement: The dev guard knows about native writes
In dev, the check that warns "system X wrote to C, which no query of it declares Mut" SHALL NOT warn for a component that an exported param of that system may write.

#### Scenario: A native write to a read column
- **WHEN** a TS system reads `Health` through `Query<[Health]>` and calls a method whose param holds `Query<&mut Health>` and writes it
- **THEN** no warning is logged

### Requirement: An export may stand for a Bevy spelling
`#[export]` SHALL accept `stands_for = "<spelling>"`, one `Res<..>` or `ResMut<..>` spelling per export. A TS system param written with that spelling SHALL be read as the export, typed as its class, and called as it. A second export claiming the same spelling SHALL fail registration naming both Rust types. Resolution SHALL be: the `Time<..>` route first (script-time), then claimed spellings, then generic `Res`/`ResMut` (rust-resources); an unclaimed spelling SHALL behave as today. The handshake's `ExportedParam` SHALL carry `standsFor` when set, an addition in the next free protocol minor.

#### Scenario: Pausing virtual time
- **WHEN** script-time exports `VirtualTime` with `stands_for = "ResMut<Time<Virtual>>"`, and a TS system takes `time: ResMut<Time<Virtual>>` and calls `time.pause()`
- **THEN** it type-checks, loads, and `Time<Virtual>` is paused after the call

#### Scenario: Two claims
- **WHEN** two crates export params both with `stands_for = "ResMut<Time<Virtual>>"`
- **THEN** the app fails to build, naming both Rust types

### Requirement: Writing params coverage
Each Bevy writing param shape SHALL be supported, refused with the reason given, or left for the named owner, as follows, and each row SHALL have a test.

| Shape | Status | Reason or owner |
|---|---|---|
| `ResMut<T>`, `Option<ResMut<T>>` (incl. `ResMut<Assets<T>>`) | Supported | Writes in place; `None` when absent |
| `Query<&mut T, F>`, mixed tuples, `Mut<T>` | Supported | Writes in place |
| `Single<&mut T>`, `Option<Single<..>>`, `Populated<&mut T>` | Supported | Failed validation throws |
| `ParamSet<(..)>` with writes | Supported | Bevy checks the set's own conflicts |
| `MessageWriter<M>`, `MessageMutator<M>` | Supported | In place on `Messages<M>`; Bevy refuses both of one `M` in one param, so they are two exports |
| `NonSendMut<T>`, `Option<NonSendMut<T>>` | Supported | Calls run on the main thread, exclusive |
| `Commands` | Supported | Deferred, applied after the run |
| `Gizmos`, `Deferred<T>`, `ParallelCommands` | Supported | Deferred, applied after the run |
| `Local<T>` | Supported | One per exported param, shared by every TS system that takes it, not one per system |
| `Query<EntityMut>`, `EntityMutExcept` | Refused at registration | Unbounded write access: the overlap check and dev guard cannot name the columns |
| `FilteredEntityMut`, `FilteredResourcesMut` | Inert | Their access comes from a builder, which an export never has: they register none, and every read or write through them fails |
| `DeferredWorld` | Refused at registration | World-wide write; its hooks and observers run synchronously inside the call |
| `&mut World`, exclusive system params | Refused at compile time | Not a `SystemParam`; can move column memory under JS |
| `ResMut<Time<Virtual>>`, `ResMut<Time<Fixed>>` as `VirtualTime`, `FixedTime` | Later | script-time, on this change's `stands_for` |
| Reflected component and resource writes from a host call | Later | reflect-coverage stage 3, on this change's host call and run ticks |

#### Scenario: Every row tested
- **WHEN** the change lands
- **THEN** each Supported row has a test that writes through it from TS and observes the result, and each Refused row has a test that sees the refusal and its message
