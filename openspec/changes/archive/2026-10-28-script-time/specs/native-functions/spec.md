## ADDED Requirements

### Requirement: Getters are properties
A `pub fn` in an `#[export]` block marked `#[getter]` SHALL take `&self` and no other argument, and SHALL become a read-only property of the TS type under its Rust name, unchanged (`is_paused`), as Bevy's getters are named on the read views (ts-systems). Each read SHALL be one synchronous host call, under "Method calls are synchronous", at the TS system's ticks. A `#[getter]` taking arguments or `&mut self` SHALL be a compile error naming it. The declaration SHALL be `readonly <name>: <type>`. The handshake's `ExportedParam` SHALL list them as `properties`, beside `methods`, an addition in the next free protocol minor.

#### Scenario: Reading a clock after a write
- **WHEN** an export has `pub fn pause(&mut self)` and `#[getter] pub fn is_paused(&self) -> bool`, and a TS system calls `time.pause()` then reads `time.is_paused`
- **THEN** it reads `true`, and `tsc` rejects `time.is_paused = false`

#### Scenario: A getter with an argument
- **WHEN** an export marks `pub fn speed(&self, scale: f32) -> f32` with `#[getter]`
- **THEN** the crate fails to compile, naming `speed`

## MODIFIED Requirements

### Requirement: Rust exports system params to TS
A Rust crate (the project's own or a port) SHALL be able to export a Bevy `SystemParam` to TS by putting `#[bevy_ts::export(ts = "Name", module = "bevy-ts/<module>")]` on an `impl` block of the param and calling `app.ts_param::<P>()`. Every `pub fn` in that block SHALL become a method of the TS type `Name`, its name in camelCase, except one marked `#[getter]` ("Getters are properties"). The macro SHALL generate the host function per method, the argument and result codecs, the TS declaration and the JS glue. Method signatures SHALL be restricted to `&self` or `&mut self` plus scalars, `bool`, `Vec2`, `Vec3`, `Quat`, `UVec2`, `UVec3`, `Entity`, `Color` (taking every form `bevy_ts::color` takes, a hex string parsed in Rust and never in JS), `Isometry2d` and `Isometry3d` (taking a vector, a rotation or `{ rotation, translation }`, as Bevy's `From` impls do), `Transform` (taking a `Transform` or a `GlobalTransform` value), `NativeLayout` records, fieldless enums (crossing as their variant names, a record that leaves one out getting its first variant), `&str` arguments and `String` results, `HostPoints<T>` of `f32`, `Vec2` or `Vec3` (an array of points or a `Float32Array`, crossing as a slice), `HostList<T>` of a tuple of these kinds (an array of arrays, crossing in the call's slots), `HostJson` values (a JSON value with a declared TS type), `&[T]` and `&mut [T]` of those numeric kinds, and `Option` of those; any other type SHALL be a compile error naming the parameter. A method MAY return its builder: the call returns the same TS object, typed as the builder's interface, whose methods are the builder's settings. The param MAY write: `ResMut`, `Query<&mut T>` and the other shapes the coverage table supports. A type that is not a `SystemParam`, such as `&mut World`, SHALL fail to compile.

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
| `ResMut<Time<Virtual>>`, `ResMut<Time<Fixed>>` as `VirtualTime`, `FixedTime` | Supported | Writes in place through `stands_for` (script-time, ts-systems "Writing the game and fixed clocks") |
| Reflected component and resource writes from a host call | Later | reflect-coverage stage 3, on this change's host call and run ticks |

#### Scenario: Every row tested
- **WHEN** the change lands
- **THEN** each Supported row has a test that writes through it from TS and observes the result, and each Refused row has a test that sees the refusal and its message
