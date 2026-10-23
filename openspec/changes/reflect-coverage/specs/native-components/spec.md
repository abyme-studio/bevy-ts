## MODIFIED Requirements

### Requirement: Layout-tier Rust components are columns in TS systems
A registered Rust component or resource with `ReflectComponent` SHALL be usable as query data in a TS system (`Query<[Mut<T>]>`, `Res<T>`) when at least one reflected leaf is a plain-data type bevy-ts knows (`f32`, `f64`, the integer types, `bool`, `Entity`, `Vec2`, `Vec3`, `Vec3A`, `Vec4`, `Quat`, fieldless enums), whatever its `repr`. Every other leaf (opaque to reflection, such as `Duration`, or not plain data) SHALL be a hole: never a column and never in `q.tables`, and a type with holes SHALL NOT offer raw `q.tables` writes. A hole SHALL be readable and writable through reflection in the shape `bevy_ts::reflect_value` gives its kind: as a TSX prop, in a `.of({...})` value a command inserts, and as a getter and setter on a TS system's row (stage 3, see ts-systems). A hole whose type is on the uncovered list SHALL fail to load or to write with the list's reason. The engine SHALL fill bridge's native layout registry at startup from reflection: from a value of the type (`ReflectDefault`, `ReflectFromWorld` or the first live row), each leaf's offset is its reference's address minus the value's address. A type whose derived offsets fall outside `size_of::<T>()`, overlap or disagree with their leaf sizes SHALL be reflect tier, with the reason in `bevy-ts types`. A component without `Reflect` MAY derive `bevy_ts_native::NativeLayout`, which fills the registry from `core::mem::offset_of!` and needs public fields but no `repr(C)`. The component's TS name SHALL be its reflected type path.

#### Scenario: Rapier's Velocity written by TS
- **WHEN** the project adopts `bevy_rapier3d` and a TS system queries `Query<[Mut<Velocity>]>` and writes `v.linear.x`
- **THEN** Rapier's own `Velocity` changes that frame with no mirror type and no `repr(C)`, and `Changed<Velocity>` holds for exactly the rows TS wrote

#### Scenario: Time with opaque Durations
- **WHEN** a TS system takes `time: Res<Time>`
- **THEN** it reads `time.delta_secs` and `time.elapsed_secs` as columns, and `time.delta` (a `Duration`) as seconds through reflection, never through `q.tables`

#### Scenario: A private reflected field
- **WHEN** a reflected component's field is private (`LockedAxes(u8)`)
- **THEN** it is still layout tier; reflection gives its offset where `offset_of!` cannot

#### Scenario: A warm sun
- **WHEN** a scene has `<DirectionalLight illuminance={8000} color={hex("#ffd7a8")} />` and a reload changes the colour to `srgb(1, 0.9, 0.8)`
- **THEN** the light is inserted with `illuminance` written from bytes and `color` from reflection, and the reload writes only `color`

#### Scenario: A sun's colour read by a system
- **WHEN** a TS system with `Query<[Mut<DirectionalLight>]>` reads `light.color` and sets it to `srgb(1, 0, 0)`
- **THEN** it reads `{ Srgba: { red, green, blue, alpha } }`, the light is red that frame, `Changed<DirectionalLight>` holds for that row, and `q.tables` has no `color` array

#### Scenario: An opaque hole
- **WHEN** a scene sets a hole whose type is on the uncovered list (an opaque type with no reflected serde)
- **THEN** the scene fails to load with an error naming the field and the list's reason; `<ScreenSpaceTransmission steps={4} />` (a `usize`) now loads

### Requirement: Reflect-tier Rust components are typed but never columns
Any registered component or resource with `ReflectComponent` or `ReflectResource` that has no plain-data leaf (or no native layout) SHALL be visible to TS as a typed value in the shape `bevy_ts::reflect_value` gives it: usable as a TSX component element, in `.of(...)` values that commands spawn and insert, as query data and `Res`/`ResMut` data in TS systems (stage 3, see ts-systems), in `With` and `Without` filters, in `NodeRef.get`, and in `bevy-ts/` protocol methods. It SHALL never be a column: its rows are read and written through reflection.

#### Scenario: A component holding a Handle
- **WHEN** the native crate or an adopted plugin registers `struct Weapon { mesh: Handle<Mesh>, name: String }` with `Reflect`
- **THEN** `<Weapon name="laser" />` type-checks in TSX, `commands.spawn(Weapon.of({ mesh: "models/laser.glb#Mesh0", name: "laser" }))` inserts it, and a TS system with `Query<[Weapon]>` reads `w.mesh` as `"models/laser.glb#Mesh0"` and `w.name` as `"laser"`

#### Scenario: Filtering by a reflect-tier component
- **WHEN** a TS system queries `Query<[Mut<Transform>], With<Weapon>>`
- **THEN** it runs over entities that carry `Weapon`, without reading `Weapon`'s data

### Requirement: TS types are generated from the engine's type registry
`bevy-ts types` SHALL run the engine headless with no window and no GPU, walk its `TypeRegistry`, and write `.bevy-ts/types/native.d.ts` declaring a TS handle for every component and resource registered for TS. Layout-tier fields SHALL take the bridge field types; every other field and every reflect-tier type SHALL take the TS type of its kind's shape in `bevy_ts::reflect_value` (tuples as tuple types, lists and sets as arrays, string-keyed maps as `Record<string, V>`, other maps as `[K, V][]`, enums as unions, `Option<T>` as `T | null`, `Color` as `ColorValue`, `Duration` as `number`, 64- and 128-bit integers as `number | string`, `Handle<T>` as `string | AssetRef<T> | AssetValue<T> | null`, `Entity` as `Entity`). No covered type SHALL type as `unknown`. A layout-tier class SHALL declare its holes as optional fields that `q.tables` does not carry. The prebuilt engine's own types (Bevy's components such as `Transform`, `PointLight`, `Text`) SHALL ship pre-generated in the npm package as `bevy-ts/bevy`. Generation SHALL be deterministic so a regenerate-and-diff check can run in CI. `bevy-ts dev` SHALL regenerate the file after each project-engine build.

#### Scenario: Types after a Rust change
- **WHEN** a field `boost: f32` is added to `Thrust` and the project engine rebuilds under `bevy-ts dev`
- **THEN** `native.d.ts` gains `boost: number` on `Thrust`, and `tsc` accepts `thrust.boost`

#### Scenario: Determinism
- **WHEN** `bevy-ts types` runs twice on the same engine
- **THEN** the two outputs are byte-identical

#### Scenario: Typed colours and projections
- **WHEN** a script writes `<PointLight color={hsl(30, 0.8, 0.6)} />` and `<Projection value={{ Orthographic: { scale: 2 } }} />`
- **THEN** `tsc` accepts both, and rejects `<PointLight color={3} />` and `<Projection value={{ Isometric: {} }} />`

#### Scenario: Typed collections
- **WHEN** the native crate registers `struct Inventory { items: Vec<String>, counts: HashMap<String, u32>, cooldown: Duration, owner: Option<Entity> }`
- **THEN** `native.d.ts` types it `{ items: string[]; counts: Record<string, number>; cooldown: number; owner: Entity | null }`

## ADDED Requirements

### Requirement: One JSON shape per reflect kind, read and written by one owner
`bevy_ts::reflect_value` SHALL own the JSON shape of every reflect kind, with `read(registry, value)` and `write(registry, server, slot, base, json)` as inverses: for every covered type, writing the JSON `read` gives over a default SHALL give a value `reflect_partial_eq` to the one read. Its kinds SHALL be structs, tuple structs and newtypes, tuples, arrays, lists (`Vec`, `VecDeque`), sets (`HashSet`, `BTreeSet`), maps (`HashMap`, `BTreeMap`), enums of every variant kind, `Option`, `Color` in every space, glam and Bevy math types (vectors of every scalar, `Quat`, matrices, affines, isometries, rotations, directions), `Duration`, integers of every width, `Handle<T>`, `Entity`, opaque types with reflected serde, and dynamic values representing a registered type. Its `Handle<T>` arm SHALL call one optional `HandleSlots` hook, for both read and write, that another module installs: `read` SHALL try the hook before the asset path, and `write` SHALL pass the hook any JSON that is not a string or `null`. With no hook installed, a handle SHALL read as its asset path or `null` and write from a path or `null`, and any other JSON SHALL fail naming the handle type. A list, set or map SHALL be written whole, not merged. A set and a non-string-keyed map SHALL read in a deterministic order. TSX, TS systems and the protocol SHALL read and write reflected values only through it.

#### Scenario: A round trip of every kind
- **WHEN** a test component holds one field of each kind above with non-default values, and its `read` JSON is written into a fresh default
- **THEN** the result is `reflect_partial_eq` to the original

#### Scenario: Big integers
- **WHEN** a `u64` field holds `2^60`
- **THEN** it reads as the string `"1152921504606846976"`, and writing that string or a number within ±2^53 both succeed

#### Scenario: A handle with no hook
- **WHEN** no `HandleSlots` hook is installed and TSX writes `mesh={{ $asset: 3 }}` into a `Handle<Mesh>` field
- **THEN** the load fails naming `Handle<Mesh>`, and `mesh="models/a.glb#Mesh0"` loads that path

#### Scenario: A list is replaced
- **WHEN** a component's `items: Vec<String>` is `["a", "b"]` and TSX writes `items={["c"]}`
- **THEN** `items` is `["c"]`

### Requirement: No reflected type has an unchecked gap
A test SHALL walk every component and resource registered with `ReflectComponent` or `ReflectResource` in the prebuilt engine plus the Rapier port, recursively through every field type, and for each type check that `read` succeeds, `write` round-trips, and the generated TS type is not `unknown`. A type that fails SHALL fail the test with its type path, field path and kind, unless `reflect_value`'s uncovered list names it with a reason. The test SHALL also fail when the uncovered list names a type the walk never meets. `docs/reflection.md` SHALL hold the coverage matrix: each reflect kind with its read, write, TS type and the test that proves it, and the uncovered list with reasons.

#### Scenario: A new gap
- **WHEN** a Bevy upgrade adds a component with a field of a kind `reflect_value` does not handle
- **THEN** the hole check fails naming that component, field and kind

#### Scenario: A stale allowlist entry
- **WHEN** the uncovered list names a type no registered component uses
- **THEN** the hole check fails naming the entry
