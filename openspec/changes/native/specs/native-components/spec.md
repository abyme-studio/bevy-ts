## ADDED Requirements

### Requirement: Layout-tier Rust components are columns in TS systems
A Rust component SHALL be usable in a TS system's `read` or `write` when it derives `bevy_ts_native::NativeLayout` and is registered with `app.ts_component::<T>()`. The derive SHALL require `#[repr(C)]` and SHALL accept only fields whose types map to bridge field types (`f32`, `f64`, the integer types, `bool`, `Vec2`, `Vec3`, `Vec4`, `Quat`, `Entity`, fixed arrays of these, and fieldless `#[repr(u8|u16|u32)]` enums), and SHALL fill bridge's native layout registry from `core::mem::offset_of!`, `size_of` and `align_of`. A field of any other type SHALL be a compile error naming the field. The component's TS name SHALL be its reflected type path unless `#[native_layout(name = "...")]` overrides it.

#### Scenario: A Rust component written by TS
- **WHEN** the native crate declares `#[derive(Component, Reflect, NativeLayout)] #[repr(C)] struct Thrust { force: Vec3, fuel: f32 }`, registers it, and a TS system writes `thrust.fuel -= dt`
- **THEN** a Rust system reading `Thrust` sees the new fuel that frame, and `Changed<Thrust>` holds for exactly the rows TS wrote

#### Scenario: A String field
- **WHEN** a component deriving `NativeLayout` has a `name: String` field
- **THEN** the native crate fails to compile with an error naming `name`

#### Scenario: Missing repr(C)
- **WHEN** a component derives `NativeLayout` without `#[repr(C)]`
- **THEN** the native crate fails to compile with an error saying `repr(C)` is required

### Requirement: Reflect-tier Rust components are typed but never columns
Any registered component with `ReflectComponent` and no native layout SHALL be visible to TS as a typed value: usable as a TSX component element, in `bevy-ts/` protocol methods and through bridge's reflected access for plugins, read and written as JSON through Bevy reflection. TS systems SHALL reject it in `read` and `write` (bridge ts-components) and SHALL accept it in `with` and `without`.

#### Scenario: A component holding a Handle
- **WHEN** the native crate registers `struct Weapon { mesh: Handle<Mesh>, name: String }` with `Reflect`
- **THEN** `<Weapon name="laser" />` type-checks in TSX, and a TS system with `read: [Weapon]` fails to load with an error naming `Weapon` and saying it has no native layout

#### Scenario: Filtering by a reflect-tier component
- **WHEN** a TS system declares `with: [Weapon]` and `write: [Transform]`
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
- **THEN** a TS system reads it as an entity reference it can pass to `ctx.commands`, and writing an invalid id is rejected when Rust applies a command that uses it
