# ts-components Specification

## Purpose
How TS declares components (class-based, with native layouts), registers them as real ECS components, and reads and writes their fields in place.

## Requirements

### Requirement: Components are declared in TS with field types
The `bevy-ts` module SHALL export `Component(name, fields, options?)`, used as a class base: `export class Mover extends Component("game::Mover", { speed: f32 }) {}`. `name` is a stable path such as `game::Mover` and `fields` maps each field name to a field type. The class SHALL be the component's handle in queries (`Query<[Mover]>`), its TSX tag (`<Mover speed={2} />`) and its constructor for commands (`new Mover({ speed: 2 })`); its instance type SHALL be the component's data. `Tag(name)` SHALL declare a component with no fields. Each field type SHALL also be a TS type of the same name (`f32` is `number`), so `Local<{ timer: f32 }>` can name a layout. Field types in this change: `f32`, `f64`, `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `bool`, `vec2`, `vec3`, `vec4`, `quat`, `entity`, `enumOf(...variants)` and `array(type, n)`. A field MAY carry a default and editor hints (`min`, `max`, `step`, `label`). Strings, lists and other heap data SHALL be rejected at declaration time.

#### Scenario: A plain numeric component
- **WHEN** `game.ts` declares `class Mover extends Component("game::Mover", { speed: f32.default(1), amplitude: f32.default(2) }) {}`
- **THEN** a component named `game::Mover` with two `f32` fields exists in the world after the module loads

#### Scenario: A string field
- **WHEN** a component declares a field of type `string`
- **THEN** loading fails with an error naming the component and field

### Requirement: Native layout rules
A TS component's layout SHALL follow C struct rules: fields in declaration order, each at an offset aligned to its own alignment, total size rounded up to the largest field alignment. `vec2`/`vec3`/`vec4`/`quat` are `f32` arrays aligned to 4; `bool` is one byte holding 0 or 1; `entity` is a `u64` holding `Entity::to_bits`, aligned to 8; `enumOf` is the smallest unsigned integer holding its variants. Rust SHALL compute the layout and hand the offsets back to JS; JS SHALL NOT compute its own.

#### Scenario: Offsets of a mixed component
- **WHEN** a component declares `{ alive: bool, hp: f32, pos: vec3 }`
- **THEN** its layout is size 20, align 4, with `alive` at 0, `hp` at 4 and `pos` at 8

### Requirement: Registration as dynamic ECS components
Each TS component SHALL be registered with `World::register_component_with_descriptor` and `ComponentDescriptor::new_with_layout`: table storage, no drop function, mutable, summary tick on, default clone behaviour. Its name in Bevy SHALL be the declared name. It SHALL be queryable by Rust through its `ComponentId` and usable in `QueryBuilder`, `insert_by_id` and `get_by_id`.

#### Scenario: A Rust system reads a TS component
- **WHEN** a Rust system builds a query with `QueryBuilder::ref_id` on `game::Mover`'s `ComponentId`
- **THEN** it iterates every entity carrying `game::Mover` and reads its bytes in the declared layout

### Requirement: A component name keeps its identity
Declaring the same name with the same layout in a later generation SHALL reuse the existing `ComponentId`. Two declarations of one name in one generation SHALL fail the load.

#### Scenario: A reload without schema change
- **WHEN** `game.ts` is edited without touching `game::Mover`'s fields and reloads
- **THEN** `game::Mover` keeps its `ComponentId` and every entity keeps its value

### Requirement: Rust components cross only with a declared native layout
A Rust component SHALL be usable as query data in a TS system only if a native layout for it is registered in Rust: from reflection, when every reflected leaf is plain data (offsets from a value's field references, bounds-checked, operator ruling 2026-10-20, native decision 5), or from `core::mem::offset_of!` for a `NativeLayout` derive, with the type's real size and alignment either way. `Transform` (`translation: vec3`, `rotation: quat`, `scale: vec3`) SHALL take the reflection path like any other type and be exposed to TS as `Transform` from `bevy-ts/bevy`. A Rust component with no native layout SHALL be rejected as query data with an error naming it; it MAY be used in `With`/`Without` filters.

#### Scenario: Moving a Transform from TS
- **WHEN** a TS system queries `Mut<Transform>` and adds to `translation.y`
- **THEN** Bevy's own `Transform` of that entity changes and the rendered cube moves

#### Scenario: A Rust component without a layout
- **WHEN** a TS system queries `GlobalTransform` and no layout is registered for it
- **THEN** loading fails with an error naming `GlobalTransform`

### Requirement: Generated field accessors
For each component, the `bevy-ts` module SHALL give TS code a typed view object whose properties read and write the fields of one row in place (`mover.speed`, `t.translation.y`), plus the raw offsets and size for code that indexes the typed arrays itself. Every setter SHALL also store the run's change tick in the row's changed tick (see ts-systems). TS types for declared components SHALL be inferred from the `Component()` call without a codegen step. A component read by a query SHALL be typed read-only: its fields `readonly`, its vector fields `ReadonlyVec3`/`ReadonlyQuat` without mutating methods. A written component (`Mut<C>`) SHALL type its scalar fields as assignable and its vector fields as views that are not reassignable and are mutated in place with glam-style methods that write `this` and allocate nothing (`set`, `copy`, `add`, `sub`, `scale`, `addScaled`, `normalize`, `lerp`, `subVectors`, `rotate`, ...). `vec3(x, y, z)` and `quat()` SHALL make scratch values with the same methods.

#### Scenario: Typed access in the editor
- **WHEN** a system's `run` writes `mover.sped = 2`
- **THEN** `tsc` reports that `sped` does not exist on the component's type

#### Scenario: Writing a read component
- **WHEN** a system with `q: Query<[Mover]>` writes `mover.speed = 2`
- **THEN** `tsc` reports that `speed` is read-only
