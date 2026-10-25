## MODIFIED Requirements

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
- **THEN** Rust receives the colour `bevy_ts::color` gives for each form, and `"#ff00"` (no such form) throws naming the method, the argument and the forms taken

#### Scenario: An isometry from a translation
- **WHEN** a method takes `isometry: Isometry3d` and TS passes `[1, 2, 3]`
- **THEN** Rust receives `Isometry3d::from_translation(Vec3::new(1.0, 2.0, 3.0))`

#### Scenario: Points as an array or a Float32Array
- **WHEN** a method takes `positions: HostPoints<Vec3>` and TS passes `[[0, 0, 0], [1, 0, 0]]`, then `new Float32Array([0, 0, 0, 1, 0, 0])`
- **THEN** Rust receives the same two points both times, and a `Float32Array` of 4 floats throws naming the method and argument

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
