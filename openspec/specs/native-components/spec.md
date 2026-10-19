# native-components Specification

## Purpose
How Rust components reach TS: plain-data fields read from reflection as zero-copy columns (holes for the rest), reflect-tier types as typed values for TSX, the protocol and filters, and TS types generated from the engine's registry.

## Requirements

### Requirement: Layout-tier Rust components are columns in TS systems
A registered Rust component or resource with `ReflectComponent` SHALL be usable as query data in a TS system (`Query<[Mut<T>]>`, `Res<T>`) when at least one reflected leaf is a plain-data type bevy-ts knows (`f32`, `f64`, the integer types, `bool`, `Entity`, `Vec2`, `Vec3`, `Vec3A`, `Vec4`, `Quat`, fieldless enums), whatever its `repr`. Every other leaf (opaque to reflection, such as `Duration`, or not plain data) SHALL be a hole: never exposed to TS and never written, and a type with holes SHALL NOT offer raw `q.tables` writes. The engine SHALL fill bridge's native layout registry at startup from reflection: from a value of the type (`ReflectDefault`, `ReflectFromWorld` or the first live row), each leaf's offset is its reference's address minus the value's address. A type whose derived offsets fall outside `size_of::<T>()`, overlap or disagree with their leaf sizes SHALL be reflect tier, with the reason in `bevy-ts types`. A component without `Reflect` MAY derive `bevy_ts_native::NativeLayout`, which fills the registry from `core::mem::offset_of!` and needs public fields but no `repr(C)`. The component's TS name SHALL be its reflected type path.

#### Scenario: Rapier's Velocity written by TS
- **WHEN** the project adopts `bevy_rapier3d` and a TS system queries `Query<[Mut<Velocity>]>` and writes `v.linear.x`
- **THEN** Rapier's own `Velocity` changes that frame with no mirror type and no `repr(C)`, and `Changed<Velocity>` holds for exactly the rows TS wrote

#### Scenario: Time with opaque Durations
- **WHEN** a TS system takes `time: Res<Time>`
- **THEN** it reads `time.delta_secs` and `time.elapsed_secs` from Bevy's own `Time`, and its `Duration` fields are not visible to TS

#### Scenario: A private reflected field
- **WHEN** a reflected component's field is private (`LockedAxes(u8)`)
- **THEN** it is still layout tier; reflection gives its offset where `offset_of!` cannot

### Requirement: Layout-tier writes stay valid Rust values
`bool`, fieldless-enum and `Entity` fields of a layout-tier Rust type SHALL be written only through accessor setters, which write `0`/`1`, a variant byte read from this build at startup, or a validated entity id; `q.tables` SHALL NOT expose them as raw arrays. A component declared immutable SHALL NOT be writable from TS. Fields marked `#[reflect(ignore)]` SHALL never be read or written.

#### Scenario: An enum field
- **WHEN** a TS system sets `body.value = "Fixed"` on Rapier's `RigidBody`
- **THEN** the byte written is the one this build uses for `RigidBody::Fixed`

### Requirement: Reflect-tier Rust components are typed but never columns
Any registered component with `ReflectComponent` that has no plain-data leaf (or no native layout) SHALL be visible to TS as a typed value: usable as a TSX component element, in `bevy-ts/` protocol methods and through bridge's reflected access for plugins, read and written as JSON through Bevy reflection. TS systems SHALL reject it as query data (ts-components) and SHALL accept it in `With` and `Without` filters.

#### Scenario: A component holding a Handle
- **WHEN** the native crate or an adopted plugin registers `struct Weapon { mesh: Handle<Mesh>, name: String }` with `Reflect`
- **THEN** `<Weapon name="laser" />` type-checks in TSX, and a TS system with `Query<[Weapon]>` fails to load with an error naming `Weapon` and saying it has no native layout

#### Scenario: Filtering by a reflect-tier component
- **WHEN** a TS system queries `Query<[Mut<Transform>], With<Weapon>>`
- **THEN** it runs over entities that carry `Weapon`, without access to `Weapon`'s data

### Requirement: TS types are generated from the engine's type registry
`bevy-ts types` SHALL run the engine headless with no window and no GPU, walk its `TypeRegistry`, and write `.bevy-ts/types/native.d.ts` declaring a TS handle for every component registered for TS, with field types (layout tier: the bridge field types; reflect tier: the JSON shape of `registry.schema`, with `Handle<T>` as an asset path string and `Entity` as an entity reference). The prebuilt engine's own types (Bevy's components such as `Transform`, `PointLight`, `Text`) SHALL ship pre-generated in the npm package as `bevy-ts/bevy`. Generation SHALL be deterministic so a regenerate-and-diff check can run in CI. `bevy-ts dev` SHALL regenerate the file after each project-engine build.

#### Scenario: Types after a Rust change
- **WHEN** a field `boost: f32` is added to `Thrust` and the project engine rebuilds under `bevy-ts dev`
- **THEN** `native.d.ts` gains `boost: number` on `Thrust`, and `tsc` accepts `thrust.boost`

#### Scenario: Determinism
- **WHEN** `bevy-ts types` runs twice on the same engine
- **THEN** the two outputs are byte-identical

### Requirement: Non-POD data has a stated path
A component with heap or shared data (`String`, `Vec`, `Handle<T>`, `Box<dyn T>`, `HashMap`) SHALL be a Rust type in the native crate; TS SHALL NOT declare one (bridge rejects such fields). `Entity` references SHALL be layout-tier fields. An author who needs a TS system to iterate per-row heap data SHALL split it: the fixed-size part as a layout-tier component, the rest as a reflect-tier component on the same entity.

#### Scenario: Entity references from TS
- **WHEN** a layout-tier component has a field `target: Entity`
- **THEN** a TS system reads it as an entity reference it can pass to its `Commands` param, and writing an invalid id is rejected when Rust applies a command that uses it
