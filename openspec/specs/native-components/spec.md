# native-components Specification

## Purpose
How Rust components reach TS: plain-data fields read from reflection as zero-copy columns (holes for the rest), reflect-tier types as typed values for TSX, the protocol and filters, and TS types generated from the engine's registry.

## Requirements

### Requirement: Layout-tier Rust components are columns in TS systems
A registered Rust component or resource with `ReflectComponent` SHALL be usable as query data in a TS system (`Query<[Mut<T>]>`, `Res<T>`) when at least one reflected leaf is a plain-data type bevy-ts knows (`f32`, `f64`, the integer types, `bool`, `Entity`, `Vec2`, `Vec3`, `Vec3A`, `Vec4`, `Quat`, fieldless enums), whatever its `repr`. Every other leaf (opaque to reflection, such as `Duration`, or not plain data) SHALL be a hole: never exposed to TS systems and never written from them, and a type with holes SHALL NOT offer raw `q.tables` writes. A hole reflection can write (a `Color`, an enum with data, an `Option`, a nested struct) SHALL be a TSX prop, written through reflection; an opaque hole SHALL NOT. The engine SHALL fill bridge's native layout registry at startup from reflection: from a value of the type (`ReflectDefault`, `ReflectFromWorld` or the first live row), each leaf's offset is its reference's address minus the value's address. A type whose derived offsets fall outside `size_of::<T>()`, overlap or disagree with their leaf sizes SHALL be reflect tier, with the reason in `bevy-ts types`. A component without `Reflect` MAY derive `bevy_ts_native::NativeLayout`, which fills the registry from `core::mem::offset_of!` and needs public fields but no `repr(C)`. The component's TS name SHALL be its reflected type path.

#### Scenario: Rapier's Velocity written by TS
- **WHEN** the project adopts `bevy_rapier3d` and a TS system queries `Query<[Mut<Velocity>]>` and writes `v.linear.x`
- **THEN** Rapier's own `Velocity` changes that frame with no mirror type and no `repr(C)`, and `Changed<Velocity>` holds for exactly the rows TS wrote

#### Scenario: Time with opaque Durations
- **WHEN** a TS system takes `time: Res<Time>`
- **THEN** it reads `time.delta_secs` and `time.elapsed_secs` from Bevy's own `Time`, and its `Duration` fields are not visible to TS

#### Scenario: A script insert keeps the holes' defaults
- **WHEN** a TS system runs `commands.entity(camera).insert(AmbientLight.of({ brightness: 400 }))`
- **THEN** the camera's `AmbientLight` has brightness 400 and its `Default` for every field the script did not give, so its colour hole stays white and the scene is lit by 400

#### Scenario: A private reflected field
- **WHEN** a reflected component's field is private (`LockedAxes(u8)`)
- **THEN** it is still layout tier; reflection gives its offset where `offset_of!` cannot

#### Scenario: A warm sun
- **WHEN** a scene has `<DirectionalLight illuminance={8000} color={hex("#ffd7a8")} />` and a reload changes the colour to `srgb(1, 0.9, 0.8)`
- **THEN** the light is inserted with `illuminance` written from bytes and `color` from reflection, the reload writes only `color`, and a TS system querying `DirectionalLight` still sees no `color` field

#### Scenario: A soft-shadowed sun
- **WHEN** a scene has `<DirectionalLight shadow_maps_enabled soft_shadow_size={0.5} />` and a reload changes it to `soft_shadow_size={null}`
- **THEN** the prebuilt engine, which has Bevy's `experimental_pbr_pcss`, writes the `Option` hole through reflection: `Some(0.5)`, then `None`, and `tsc` types the prop `number | null`

#### Scenario: An opaque hole
- **WHEN** a scene sets `<ScreenSpaceTransmission steps={4} />`
- **THEN** the scene fails to load with an error naming `steps` as opaque to reflection

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
`bevy-ts types` SHALL run the engine headless with no window and no GPU, walk its `TypeRegistry`, and write `.bevy-ts/types/native.d.ts` declaring a TS handle for every component registered for TS, with field types (layout tier: the bridge field types; reflect tier: the JSON shape of `registry.schema`, with `Handle<T>` as an asset path string and `Entity` as an entity reference). A `Color` SHALL type as `ColorValue` wherever it appears. A layout-tier class SHALL declare its writable holes as optional TSX props that TS system rows do not carry. A reflect-tier component that is an enum SHALL type its TSX `value` prop as the enum's shape. The prebuilt engine's own types (Bevy's components such as `Transform`, `PointLight`, `Text`) SHALL ship pre-generated in the npm package as `bevy-ts/bevy`. Generation SHALL be deterministic so a regenerate-and-diff check can run in CI. `bevy-ts dev` SHALL regenerate the file after each project-engine build.

#### Scenario: Types after a Rust change
- **WHEN** a field `boost: f32` is added to `Thrust` and the project engine rebuilds under `bevy-ts dev`
- **THEN** `native.d.ts` gains `boost: number` on `Thrust`, and `tsc` accepts `thrust.boost`

#### Scenario: Determinism
- **WHEN** `bevy-ts types` runs twice on the same engine
- **THEN** the two outputs are byte-identical

#### Scenario: Typed colours and projections
- **WHEN** a script writes `<PointLight color={hsl(30, 0.8, 0.6)} />` and `<Projection value={{ Orthographic: { scale: 2 } }} />`
- **THEN** `tsc` accepts both, and rejects `<PointLight color={3} />` and `<Projection value={{ Isometric: {} }} />`

### Requirement: Non-POD data has a stated path
A component with heap or shared data (`String`, `Vec`, `Handle<T>`, `Box<dyn T>`, `HashMap`) SHALL be a Rust type in the native crate; TS SHALL NOT declare one (bridge rejects such fields). `Entity` references SHALL be layout-tier fields. An author who needs a TS system to iterate per-row heap data SHALL split it: the fixed-size part as a layout-tier component, the rest as a reflect-tier component on the same entity.

#### Scenario: Entity references from TS
- **WHEN** a layout-tier component has a field `target: Entity`
- **THEN** a TS system reads it as an entity reference it can pass to its `Commands` param, and writing an invalid id is rejected when Rust applies a command that uses it

### Requirement: TSX writes reflected enums in Bevy's shape
A TSX prop whose reflected type is an enum SHALL take Bevy's own shape: a unit variant as its name string, and a variant with data as a one-key object `{ Variant: data }` (a newtype variant's data is its inner value; a struct variant's is an object of its fields; a tuple variant's is an array). This SHALL hold at any depth, for enums nested in structs and in other enums. An `Option<T>` SHALL take `null` for `None` and a `T` for `Some`. A reflect-tier component that is itself an enum SHALL take its whole value as the `value` prop. Choosing a variant other than the current one SHALL switch the component to it.

#### Scenario: An orthographic camera in TSX
- **WHEN** a scene has `<Camera3d /><Projection value={{ Orthographic: { scale: 1, scaling_mode: { FixedVertical: { viewport_height: 12 } } } }} />`
- **THEN** the entity's `Projection` is `Orthographic` with those fields, and its clip-from-view matrix is orthographic (its last row is `[0, 0, 0, 1]`)

#### Scenario: An orthographic camera renders
- **WHEN** the scene above looks at two equal cubes at different depths and is drawn to an offscreen target (macOS, no window shown)
- **THEN** both cubes appear the same size in the picture, where the perspective default draws the far one smaller

#### Scenario: A unit variant
- **WHEN** a reload changes `scaling_mode` to `"WindowSize"`
- **THEN** `ScalingMode::WindowSize` is written and no other field changes

#### Scenario: An unknown variant
- **WHEN** a prop names `{ Isometric: {} }` for a `Projection`
- **THEN** the scene fails to load with an error naming the type and its variants

### Requirement: A written reflected value is the JSON over the default
When TSX writes a reflected value, keys the JSON leaves out SHALL take their default: the field's value in the component's `ReflectDefault`, or for a newly chosen variant, the variant's default (a newtype variant's inner `ReflectDefault`). A key left out SHALL NOT keep the previous value. A struct or tuple variant with no default SHALL be given whole. An unknown key SHALL fail with an error naming it. A component with no `ReflectDefault` SHALL merge over its current value instead.

#### Scenario: A field dropped from a prop
- **WHEN** `<Projection value={{ Orthographic: { scale: 2 } }} />` is edited to `<Projection value={{ Orthographic: {} }} />` and reloaded
- **THEN** `scale` is back at `OrthographicProjection`'s default

#### Scenario: A typo
- **WHEN** a prop is `{ Orthographic: { scael: 2 } }`
- **THEN** the scene fails to load with an error naming `scael` and `OrthographicProjection`

### Requirement: Colours have one parser that takes every form
bevy-ts SHALL parse every colour it takes (TSX props, intrinsic and UI colours, script-material `color` params, recipe params) with one parser, `bevy_ts::color`. It SHALL take a hex string (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`), an sRGB array `[r, g, b, a?]`, and Bevy's reflected `Color` shape for every variant (`{ Srgba: {...} }`, `{ LinearRgba: {...} }`, `{ Hsla: {...} }`, ...), with `alpha` 1 when left out and every other channel required. `bevy-ts` SHALL export `srgb(r, g, b, a?)`, `linear(r, g, b, a?)`, `hsl(h, s, l, a?)` and `hex(s)`, which build these values, and the type `ColorValue`.

#### Scenario: A light's colour changes
- **WHEN** a scene's white cube is lit only by a `<DirectionalLight color={hex("#ffffff")} />`, is drawn offscreen, then the colour is reloaded as `srgb(1, 0.6, 0.3)` and drawn again
- **THEN** the light's `color` reads back as that sRGB value, and the cube's lit pixel is warmer in the second picture (red above blue) than in the first (red and blue equal)

#### Scenario: Every form, every caller
- **WHEN** `<material color={linear(1, 0.5, 0)} />`, `<mesh color={[1, 0.5, 0]} />`, a script material's `color` param `{ Hsla: { hue: 30, saturation: 1, lightness: 0.5 } }` and `spawnNative("cuboid", { color: hsl(30, 1, 0.5) })` are given
- **THEN** each is accepted and gives the same colour as the hex form of that colour, within rounding

#### Scenario: A bad colour
- **WHEN** a colour is `{ Srgba: { red: 1 } }`
- **THEN** it fails with an error naming the missing channels `green` and `blue`

### Requirement: OcclusionCulling brings its DepthPrepass
bevy-ts SHALL register `DepthPrepass` as a required component of `OcclusionCulling` when its
`pbr` feature is on, through Bevy's `App::try_register_required_components`, so that inserting
`OcclusionCulling` on a camera by any path (TSX, a level, BRP, Rust) also inserts a default
`DepthPrepass` unless one is given. A `DepthPrepass` the caller writes SHALL be kept. bevy-ts
SHALL NOT change a camera's `Msaa`: occlusion culling runs with MSAA on. If Bevy already requires
`DepthPrepass` for `OcclusionCulling`, the registration's error SHALL be logged at debug and the
engine SHALL start as usual.

#### Scenario: OcclusionCulling alone renders
- **WHEN** a TSX camera has `<Camera3d />` and `<OcclusionCulling />` and no `<DepthPrepass />`,
  and draws to an offscreen image for 30 frames
- **THEN** the camera entity has `DepthPrepass`, no render error is logged, the app does not quit,
  and the image shows the cube in front of the camera

#### Scenario: MSAA stays on
- **WHEN** the same camera keeps Bevy's default `Msaa` (4 samples)
- **THEN** its `Msaa` is still 4 samples after 30 frames and no render error is logged

#### Scenario: A written DepthPrepass is kept
- **WHEN** a TSX camera has both `<OcclusionCulling />` and `<DepthPrepass />`
- **THEN** the camera has one `DepthPrepass`, and removing `<OcclusionCulling />` on a reload
  leaves the written `DepthPrepass` in place

### Requirement: Layout-tier Rust resources are Resource classes
A registered Rust resource SHALL be tiered as a component is (a resource is a component in Bevy 0.20), and the registry SHALL record that it is a resource: from `ReflectResource` for a reflected type, and from `app.ts_resource_layout::<T>()` (`T: NativeLayout + Resource`) for a derive-only one, the resource sibling of `ts_layout::<T>()`. A layout-tier resource SHALL reach TS as a `Resource` class, usable only as `Res<R>`, `ResMut<R>`, `Option<Res<R>>` and `Option<ResMut<R>>` (ts-systems), never as query data. `bevy-ts/bevy` SHALL ship the prebuilt engine's layout-tier resources (such as `GlobalAmbientLight` and `UiScale`) and `native.d.ts` a project's, named by the components' rule: the type's short name, prefixed by its crate when two types share it. `bevy-ts` SHALL export `nativeResource(name, fields)`, the resource sibling of `nativeComponent`, for those modules and for native crates shipping their own resources. Holes and read-only fields SHALL follow the component rules. `Time` SHALL stay the class `bevy-ts` itself exports.

#### Scenario: A Bevy resource from bevy-ts/bevy
- **WHEN** a script imports `GlobalAmbientLight` from `bevy-ts/bevy`
- **THEN** it is a `Resource` class with `brightness: f32` and the `bool` field writable, a doc note naming `color` as a hole, and `Query<[GlobalAmbientLight]>` fails the load as not a component

#### Scenario: A derive-only resource from a native crate
- **WHEN** a native crate registers `#[derive(Resource, NativeLayout)] struct Wind { speed: f32 }` with `app.ts_resource_layout::<Wind>()` and ships `class Wind extends nativeResource("game_native::Wind", { speed: f32 }) {}`
- **THEN** a TS system with `ResMut<Wind>` writes `speed` in place and a Rust system's `Res<Wind>` reads it that frame

#### Scenario: Determinism with resources
- **WHEN** `scripts/gen-bevy-types.sh --check` runs after the generated files are committed
- **THEN** it passes: the `bevy-ts/bevy` outputs, resource classes included, are byte-identical
