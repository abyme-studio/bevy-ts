## MODIFIED Requirements

### Requirement: Layout-tier Rust components are columns in TS systems
A registered Rust component or resource with `ReflectComponent` SHALL be usable as query data in a TS system (`Query<[Mut<T>]>`, `Res<T>`) when at least one reflected leaf is a plain-data type bevy-ts knows (`f32`, `f64`, the integer types, `bool`, `Entity`, `Vec2`, `Vec3`, `Vec3A`, `Vec4`, `Quat`, fieldless enums), whatever its `repr`. Every other leaf (opaque to reflection, such as `Duration`, or not plain data) SHALL be a hole: never exposed to TS systems and never written from them, and a type with holes SHALL NOT offer raw `q.tables` writes. A hole reflection can write (a `Color`, an enum with data, an `Option`, a nested struct) SHALL be a TSX prop, written through reflection; an opaque hole SHALL NOT. The engine SHALL fill bridge's native layout registry at startup from reflection: from a value of the type (`ReflectDefault`, `ReflectFromWorld` or the first live row), each leaf's offset is its reference's address minus the value's address. A type whose derived offsets fall outside `size_of::<T>()`, overlap or disagree with their leaf sizes SHALL be reflect tier, with the reason in `bevy-ts types`. A component without `Reflect` MAY derive `bevy_ts_native::NativeLayout`, which fills the registry from `core::mem::offset_of!` and needs public fields but no `repr(C)`. The component's TS name SHALL be its reflected type path.

#### Scenario: Rapier's Velocity written by TS
- **WHEN** the project adopts `bevy_rapier3d` and a TS system queries `Query<[Mut<Velocity>]>` and writes `v.linear.x`
- **THEN** Rapier's own `Velocity` changes that frame with no mirror type and no `repr(C)`, and `Changed<Velocity>` holds for exactly the rows TS wrote

#### Scenario: Time with opaque Durations
- **WHEN** a TS system takes `time: Res<Time>`
- **THEN** it reads `time.delta_secs` and `time.elapsed_secs` from Bevy's own `Time`, and its `Duration` fields are not visible to TS

#### Scenario: A private reflected field
- **WHEN** a reflected component's field is private (`LockedAxes(u8)`)
- **THEN** it is still layout tier; reflection gives its offset where `offset_of!` cannot

#### Scenario: A warm sun
- **WHEN** a scene has `<DirectionalLight illuminance={8000} color={hex("#ffd7a8")} />` and a reload changes the colour to `srgb(1, 0.9, 0.8)`
- **THEN** the light is inserted with `illuminance` written from bytes and `color` from reflection, the reload writes only `color`, and a TS system querying `DirectionalLight` still sees no `color` field

#### Scenario: An opaque hole
- **WHEN** a scene sets `<ScreenSpaceTransmission steps={4} />`
- **THEN** the scene fails to load with an error naming `steps` as opaque to reflection

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

## ADDED Requirements

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
