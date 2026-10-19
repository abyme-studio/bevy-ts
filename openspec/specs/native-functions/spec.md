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
