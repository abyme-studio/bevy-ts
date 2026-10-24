## MODIFIED Requirements

### Requirement: Rust exports system params to TS
A Rust crate (the project's own or a port) SHALL be able to export a Bevy `SystemParam` to TS by putting `#[bevy_ts::export(ts = "Name", module = "bevy-ts/<module>")]` on an `impl` block of the param and calling `app.ts_param::<P>()`. Every `pub fn` in that block SHALL become a method of the TS type `Name`. The macro SHALL generate the host function per method, the argument and result codecs, the TS declaration and the JS glue. Method signatures SHALL be restricted to `&self` or `&mut self` plus scalars, `bool`, `Vec2`, `Vec3`, `Quat`, `Entity`, `NativeLayout` records, `&str` arguments and `String` results, `&[T]` and `&mut [T]` of those numeric kinds, and `Option` of those; any other type SHALL be a compile error naming the parameter. The param MAY write: `ResMut`, `Query<&mut T>` and the other shapes the coverage table supports. A type that is not a `SystemParam`, such as `&mut World`, SHALL fail to compile.

#### Scenario: Rapier's raycast
- **WHEN** the Rapier port exports `impl ReadRapierContext { pub fn cast_ray(&self, origin: Vec3, dir: Vec3, max_toi: f32, solid: bool, f: RayFilter) -> Option<RayHit> }`
- **THEN** a TS system taking `rapier: ReadRapierContext` can call `rapier.castRay(origin, dir, 1.1, true, { excludeRigidBody: e })`, typed from the Rust signature

#### Scenario: A mutable param
- **WHEN** a crate exports `impl NavEdit { pub fn block(&mut self, cell: u32) }` on a param holding `ResMut<NavMesh>`
- **THEN** the crate compiles, and a TS system calling `nav.block(7)` finds cell 7 blocked in `NavMesh` when the call returns

## ADDED Requirements

### Requirement: Strings cross as UTF-8
An exported method SHALL be able to take `&str` and `Option<&str>` arguments and return `String` or `Option<String>`, typed `string` (and `string | undefined`) in TS. The JS glue SHALL encode a string argument as UTF-8 into a scratch byte view and Rust SHALL read it in place for the length of the call; a string argument SHALL count as one of the method's slice arguments. A string result SHALL be written by Rust into a byte arena and decoded by the glue into a JS string. A `String` SHALL NOT be a field of a `HostRecord` or a slice element. A string argument SHALL NOT be checked against the run's columns for overlap: it never views one.

#### Scenario: A path in, a name out
- **WHEN** a crate exports `pub fn suffix(&self, path: &str) -> Option<String>` and a TS system calls `p.suffix("frame_time")`
- **THEN** Rust receives `"frame_time"` and the system reads `"ms"`

#### Scenario: Not ASCII
- **WHEN** a TS system passes `"bâtiment/面"` to a `&str` argument whose method returns it unchanged
- **THEN** the system reads `"bâtiment/面"`

#### Scenario: A string in a record
- **WHEN** a crate derives `HostRecord` on a struct with a `String` field
- **THEN** it fails to compile, naming the field
