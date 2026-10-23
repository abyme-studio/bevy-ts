# native-functions Specification

## Purpose
How TS calls Rust: read-only Bevy system params exported with `#[export]`, whose methods are synchronous host calls inside a TS system's run.

## Requirements

### Requirement: Rust exports system params to TS
A Rust crate (the project's own or a port) SHALL be able to export a read-only Bevy `SystemParam` to TS by putting `#[bevy_ts::export(ts = "Name", module = "bevy-ts/<module>")]` on an `impl` block of the param and calling `app.ts_param::<P>()`. Every `pub fn` in that block SHALL become a method of the TS type `Name`. The macro SHALL generate the host function per method, the argument and result codecs, the TS declaration and the JS glue. Method signatures SHALL be restricted to `&self` plus scalars, `bool`, `Vec2`, `Vec3`, `Quat`, `Entity`, `NativeLayout` records, `&[T]` and `&mut [T]` of those, and `Option` of those; any other type SHALL be a compile error naming the parameter. `P` SHALL implement `ReadOnlySystemParam`, or the crate fails to compile.

#### Scenario: Rapier's raycast
- **WHEN** the Rapier port exports `impl ReadRapierContext { pub fn cast_ray(&self, origin: Vec3, dir: Vec3, max_toi: f32, solid: bool, f: RayFilter) -> Option<RayHit> }`
- **THEN** a TS system taking `rapier: ReadRapierContext` can call `rapier.castRay(origin, dir, 1.1, true, { excludeRigidBody: e })`, typed from the Rust signature

#### Scenario: A mutable param
- **WHEN** a crate exports an `impl` on a param holding `ResMut<NavMesh>`
- **THEN** the crate fails to compile, saying the param must be read-only

### Requirement: Exported params are system param kinds
An exported param's TS type SHALL be a system param kind the compile step reads from a system's signature (script-runtime), declared by the module that exports it. A TS system SHALL reach Rust behaviour only through such a param. A name that no loaded crate exports SHALL be a type error and a load error naming it.

#### Scenario: A misspelt param type
- **WHEN** a system takes `rapier: ReadRapierContxt`
- **THEN** `tsc` reports the type missing, and loading fails naming `ReadRapierContxt`, the file and line

### Requirement: Method calls are synchronous and read-only
A method call SHALL run synchronously inside the TS system's run and return its result in the same run, as one call from JS into Rust; a batched method (slices in, slices out) SHALL be one call for the whole batch. Record and slice arguments SHALL be read in place from JS memory, and record results SHALL be written into a Rust-owned arena viewed by JS without a copy, valid until the run ends. Rust SHALL get the param with `SystemState::get` against the world, read-only, so a method sees the system's writes made so far in the run, as an overlapping query does (ts-systems), and cannot move or free column memory. A method SHALL NOT make structural changes or call back into JS. A param that fails validation SHALL throw a JS exception naming the param and method; a panic in a method SHALL be caught and thrown as an exception naming the method.

#### Scenario: 1,000 rays
- **WHEN** a system calls `rapier.castRays(rays, true, filter, hits)` with 1,000 rays
- **THEN** the backend records one call from JS into Rust, and the system reads 1,000 results before returning

#### Scenario: Before the physics context exists
- **WHEN** a system calls `rapier.castRay(...)` on a frame where Rapier's context entity has not been spawned
- **THEN** the call throws an exception naming `ReadRapierContext` and `castRay`, and the system's commands from that run are discarded

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
