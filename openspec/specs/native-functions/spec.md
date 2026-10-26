# native-functions Specification

## Purpose
How TS calls Rust: Bevy system params exported with `#[export]`, read or write, whose methods are synchronous host calls inside a TS system's run at its own ticks; deferred state applies after the run.

## Requirements

### Requirement: Rust exports system params to TS
A Rust crate (the project's own or a port) SHALL be able to export a Bevy `SystemParam` to TS by putting `#[bevy_ts::export(ts = "Name", module = "bevy-ts/<module>")]` on an `impl` block of the param and calling `app.ts_param::<P>()`. Every `pub fn` in that block SHALL become a method of the TS type `Name`. The macro SHALL generate the host function per method, the argument and result codecs, the TS declaration and the JS glue. Method signatures SHALL be restricted to `&self` or `&mut self` plus scalars, `bool`, `Vec2`, `Vec3`, `Quat`, `UVec2`, `UVec3`, `Entity`, `Color` (taking every form `bevy_ts::color` takes, a hex string parsed in Rust and never in JS), `Isometry2d` and `Isometry3d` (taking a vector, a rotation or `{ rotation, translation }`, as Bevy's `From` impls do), `Transform` (taking a `Transform` or a `GlobalTransform` value), `NativeLayout` records, fieldless enums (crossing as their variant names, a record that leaves one out getting its first variant), `&str` arguments and `String` results, `HostPoints<T>` of `f32`, `Vec2` or `Vec3` (an array of points or a `Float32Array`, crossing as a slice), `HostList<T>` of a tuple of these kinds (an array of arrays, crossing in the call's slots), `HostJson` values (a JSON value with a declared TS type), `&[T]` and `&mut [T]` of those numeric kinds, and `Option` of those; any other type SHALL be a compile error naming the parameter. A method MAY return its builder: the call returns the same TS object, typed as the builder's interface, whose methods are the builder's settings. The param MAY write: `ResMut`, `Query<&mut T>` and the other shapes the coverage table supports. A type that is not a `SystemParam`, such as `&mut World`, SHALL fail to compile.

#### Scenario: Rapier's raycast
- **WHEN** the Rapier port exports `impl ReadRapierContext { pub fn cast_ray(&self, origin: Vec3, dir: Vec3, max_toi: f32, solid: bool, f: RayFilter) -> Option<RayHit> }`
- **THEN** a TS system taking `rapier: ReadRapierContext` can call `rapier.castRay(origin, dir, 1.1, true, { excludeRigidBody: e })`, typed from the Rust signature

#### Scenario: A mutable param
- **WHEN** a crate exports `impl NavEdit { pub fn block(&mut self, cell: u32) }` on a param holding `ResMut<NavMesh>`
- **THEN** the crate compiles, and a TS system calling `nav.block(7)` finds cell 7 blocked in `NavMesh` when the call returns

#### Scenario: A fieldless enum
- **WHEN** a crate exports a method taking a `#[derive(HostRecord)]` record whose field is a fieldless enum `RayCastVisibility { VisibleInView, Visible, Any }`
- **THEN** TS declares `type RayCastVisibility = "VisibleInView" | "Visible" | "Any"`, a call passing `"Any"` reaches Rust as `Any`, one leaving the field out reaches it as `VisibleInView`, and any other string throws

#### Scenario: A colour in each form
- **WHEN** a method takes `color: Color` and TS passes `"#ff0000"`, `[1, 0, 0]`, `srgb(1, 0, 0)` and `{ LinearRgba: { red: 1, green: 0, blue: 0 } }`
- **THEN** Rust receives the colour `bevy_ts::color` gives for each form, and `"#ff000"` (no such form) throws naming the method, the argument and the forms taken

#### Scenario: An isometry from a translation
- **WHEN** a method takes `isometry: Isometry3d` and TS passes `[1, 2, 3]`
- **THEN** Rust receives `Isometry3d::from_translation(Vec3::new(1.0, 2.0, 3.0))`

#### Scenario: Points as an array or a Float32Array
- **WHEN** a method takes `positions: HostPoints<Vec3>` and TS passes `[[0, 0, 0], [1, 0, 0]]`, then `new Float32Array([0, 0, 0, 1, 0, 0])`
- **THEN** Rust receives the same two points both times, and a `Float32Array` of 4 floats throws naming the method and argument

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
`#[export]`, and an `ExportParam` written by hand, SHALL accept `stands_for = "<spelling>"`, one `Res<..>`, `ResMut<..>`, `Gizmos` or `Gizmos<..>` spelling per export. A TS system param written with that spelling SHALL be read as the export, typed as its class, and called as it. A second export claiming the same spelling SHALL fail registration naming both Rust types. Resolution SHALL be: the `Time<..>` route first (script-time), then claimed spellings, then generic `Res`/`ResMut` (rust-resources); an unclaimed spelling SHALL behave as today, and an unclaimed `Gizmos<..>` spelling SHALL fail the load naming the spelling and the claimed `Gizmos` spellings. The handshake's `ExportedParam` SHALL carry `standsFor` when set, an addition in the next free protocol minor.

#### Scenario: Pausing virtual time
- **WHEN** script-time exports `VirtualTime` with `stands_for = "ResMut<Time<Virtual>>"`, and a TS system takes `time: ResMut<Time<Virtual>>` and calls `time.pause()`
- **THEN** it type-checks, loads, and `Time<Virtual>` is paused after the call

#### Scenario: Two claims
- **WHEN** two crates export params both with `stands_for = "ResMut<Time<Virtual>>"`
- **THEN** the app fails to build, naming both Rust types

#### Scenario: A gizmo group spelling
- **WHEN** a TS system takes `gizmos: Gizmos<LightGizmoConfigGroup>`
- **THEN** it loads as the export claiming `Gizmos<LightGizmoConfigGroup>`, and `gizmos: Gizmos<NoSuchGroup>` fails the load naming `Gizmos<NoSuchGroup>` and the claimed `Gizmos` spellings

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

### Requirement: Cameras converts between world, viewport and NDC
The engine's `Cameras` param (module `bevy-ts/camera`) SHALL have, beside world-to-screen's methods, one method per Bevy 0.20 `Camera` conversion, each over the camera's `GlobalTransform` as last propagated, in logical pixels from the viewport's top-left where a viewport point is involved:

- `worldToViewportWithDepth(camera: Entity, point: Vec3): Vec3 | undefined`, `Camera::world_to_viewport_with_depth`, `z` the view-space depth.
- `viewportToWorld(camera: Entity, point: Vec2): Ray3d | undefined`, `Camera::viewport_to_world`, a ray from the near plane with a unit `direction`.
- `viewportToWorld2d(camera: Entity, point: Vec2): Vec2 | undefined`, `Camera::viewport_to_world_2d`.
- `worldToNdc(camera: Entity, point: Vec3): Vec3 | undefined` and `ndcToWorld(camera: Entity, ndc: Vec3): Vec3 | undefined`, `Camera::world_to_ndc` and `ndc_to_world`.
- `viewportToNdc(camera: Entity, point: Vec2): Vec2 | undefined`, `Camera::viewport_to_ndc`.
- `viewportsToWorld(camera: Entity, points: Float32Array, out: Float32Array): number`, one call for many points: two floats in and six out (origin, direction) per point, `NaN` six times for a miss, returning how many converted.

`Ray3d` SHALL be a record `{ origin: Vec3, direction: Vec3 }`, Bevy's `Ray3d`. Every Bevy conversion error or `None` SHALL be `undefined`. An `Entity` that is not a camera SHALL throw, naming `Cameras`, the method and the entity. `viewportsToWorld` SHALL throw when `points` is not a multiple of two floats or `out` holds fewer than six per point, as `worldsToViewport`.

#### Scenario: The centre pixel's ray
- **WHEN** a headless 960x540 engine has a camera at `[0, 0, 10]` looking at the origin, and a system calls `cameras.viewportToWorld(cam, [480, 270])`
- **THEN** it returns a ray whose `direction` is `[0, 0, -1]` and whose `origin` lies on the line from the camera through the origin, within 1e-4

#### Scenario: Round trips
- **WHEN** the system projects a world point in view with `worldToNdc` and converts back with `ndcToWorld`, and projects one with `worldToViewport` and converts that with `viewportToNdc`
- **THEN** `ndcToWorld` returns the point, and `viewportToNdc` equals `worldToNdc`'s `x` and `y`, within 1e-4

#### Scenario: Depth
- **WHEN** the system calls `worldToViewportWithDepth(cam, [0, 0, 0])`
- **THEN** it returns `[480, 270, z]` with `z` the camera's distance to the origin, 10, within a pixel and 1e-3

#### Scenario: Before the viewport has a size
- **WHEN** a system calls `viewportToWorld` on the frame the camera is spawned
- **THEN** it returns `undefined`, and a later frame returns a ray

#### Scenario: Many rays in one call
- **WHEN** a system calls `cameras.viewportsToWorld(cam, points, out)` with two points
- **THEN** the backend records one host call, it returns `2`, and `out` holds each point's origin and direction

#### Scenario: Not a camera
- **WHEN** a system calls `cameras.viewportToWorld(e, p)` with an entity that has no `Camera`
- **THEN** the call throws an exception naming `Cameras`, `viewportToWorld` and the entity

### Requirement: The engine exports MeshRayCast
Every engine (`bevy-ts dev`, `bevy-ts play`, a shipped game, `bevy-ts types`) SHALL export a param `MeshRayCast` from module `bevy-ts/camera`, over Bevy's `MeshRayCast`, with:

- `castRay(ray: Ray3d, settings: RayCastSettings | undefined, exclude: Float64Array): RayMeshHit | undefined`, the nearest hit.
- `castRayAll(ray: Ray3d, settings: RayCastSettings | undefined, exclude: Float64Array, hits: Float32Array, entity: Float64Array): number`, every hit nearest first: seven floats per hit (point, normal, distance) in `hits`, the entity's bits in `entity`; it SHALL return the number written, dropping hits past the room left and keeping the nearest, and SHALL throw when `hits` is not a multiple of seven floats.

`RayCastSettings` SHALL be `{ visibility: "Any" | "Visible" | "VisibleInView", earlyExit: boolean }`, Bevy's `RayCastVisibility` and its always or never early exit; `undefined` SHALL be Bevy's default, `VisibleInView` with early exit; a settings object that leaves `visibility` out SHALL get `VisibleInView`. `exclude` SHALL hold entity bits the cast skips, empty for none. `RayMeshHit` SHALL be `{ entity, point, normal, distance, uv?, triangleIndex? }`, with `normal` as Bevy gives it. A ray whose `direction` is zero or not finite SHALL throw, naming `MeshRayCast` and the method. A cast SHALL NOT call back into JS. A mesh SHALL be hit from the frame after it gets its `Aabb` (Bevy computes it in `PostUpdate`).

#### Scenario: The cursor's mesh
- **WHEN** a headless 960x540 engine has a camera at `[0, 0, 10]` looking at a unit cube at the origin, and a system casts `viewportToWorld(cam, [480, 270])` with `meshRayCast.castRay(ray, { visibility: "Any", earlyExit: true }, new Float64Array())`
- **THEN** it returns the cube's entity, `point` `[0, 0, 0.5]`, `normal` `[0, 0, 1]` and `distance` from the ray's origin, within 1e-3

#### Scenario: A miss
- **WHEN** the system casts a ray that passes beside every mesh
- **THEN** `castRay` returns `undefined`

#### Scenario: Every hit, nearest first
- **WHEN** two cubes lie on the ray and the system calls `castRayAll` with `earlyExit: false` and room for four hits
- **THEN** it returns `2`, the nearer cube's entity and point first

#### Scenario: Excluded entities
- **WHEN** the system casts the same ray with the nearer cube's bits in `exclude`
- **THEN** `castRay` returns the farther cube

#### Scenario: A mesh spawned this frame
- **WHEN** a cube is spawned in the frame that casts at it
- **THEN** the cast misses, and the same cast a frame later hits

#### Scenario: A bad ray
- **WHEN** a system calls `castRay` with `direction` `[0, 0, 0]`
- **THEN** the call throws an exception naming `MeshRayCast` and `castRay`

#### Scenario: Declared for TS
- **WHEN** `bevy-ts types` runs
- **THEN** module `bevy-ts/camera` declares `MeshRayCast`, `Ray3d`, `RayCastSettings` and `RayMeshHit`

### Requirement: Strings cross as UTF-8
An exported method SHALL be able to take `&str` and `Option<&str>` arguments and return `String` or `Option<String>`, typed `string` (and `string | undefined`) in TS. The JS glue SHALL encode a string argument as UTF-8 into the call's argument slots (its byte length, then its bytes), and Rust SHALL read and check it there; a string argument SHALL NOT take one of the method's slice arguments. A string result SHALL come back the same way in the result slots and be decoded by the glue into a JS string. A string too long for the call's slots SHALL throw, naming its length. A `String` SHALL NOT be a field of a `HostRecord` or a slice element. A string argument SHALL NOT be checked against the run's columns for overlap: it never views one.

#### Scenario: A path in, a name out
- **WHEN** a crate exports `pub fn suffix(&self, path: &str) -> Option<String>` and a TS system calls `p.suffix("frame_time")`
- **THEN** Rust receives `"frame_time"` and the system reads `"ms"`

#### Scenario: Not ASCII
- **WHEN** a TS system passes `"bâtiment/面"` to a `&str` argument whose method returns it unchanged
- **THEN** the system reads `"bâtiment/面"`

#### Scenario: Too long for one call
- **WHEN** a TS system passes a string of 5,000 bytes to a `&str` argument
- **THEN** the call throws, naming the string's length, and Rust is not called

#### Scenario: A string in a record
- **WHEN** a crate derives `HostRecord` on a struct with a `String` field
- **THEN** it fails to compile, naming the field

### Requirement: Slices must be their declared typed array
A slice argument `&[T]` or `&mut [T]` SHALL be given as the typed array its declaration names (`Float32Array` for `f32`, `Vec2`, `Vec3` and `Vec4`, `Float64Array` for `f64`, and so on). Any other value, including a typed array of another element type, an `ArrayBuffer` or a `DataView`, SHALL throw before the call reaches Rust, with a message naming the param, method and argument, the expected type and the type given.

#### Scenario: A Float64Array for a Float32Array
- **WHEN** a TS system calls `cameras.worldsToViewport(camera, new Float64Array(3), out)`
- **THEN** the call throws `Cameras.worldsToViewport: points must be a Float32Array, not a Float64Array`, and the method does not run

#### Scenario: A wrong out array
- **WHEN** the same call passes a `Float32Array` as `points` and a `Float64Array` as `out`
- **THEN** it throws naming `out`, and nothing is written
