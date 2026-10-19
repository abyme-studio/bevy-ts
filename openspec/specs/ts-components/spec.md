# ts-components Specification

## Purpose
TBD - created by archiving change bridge. Update Purpose after archive.

## Requirements

### Requirement: Components are declared in TS with field types
The `bevy-ts` module SHALL export `component(name, fields, options?)`, where `name` is a stable path such as `game::Mover` and `fields` maps each field name to a field type. Field types in this change: `f32`, `f64`, `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `bool`, `vec2`, `vec3`, `vec4`, `quat`, `entity`, `enumOf(...variants)` and `array(type, n)`. A field MAY carry a default and editor hints (`min`, `max`, `step`, `label`). Strings, lists and other heap data SHALL be rejected at declaration time.

#### Scenario: A plain numeric component
- **WHEN** `game.ts` declares `component("game::Mover", { speed: f32.default(1), amplitude: f32.default(2) })`
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
A Rust component SHALL be usable in a TS system's `read` or `write` only if a native layout for it is registered in Rust, with field offsets taken from `core::mem::offset_of!` and the type's real size and alignment. This change SHALL register `Transform` (`translation: vec3`, `rotation: quat`, `scale: vec3`) and expose it to TS as `Transform` from `bevy-ts/bevy`. A Rust component with no native layout SHALL be rejected in `read`/`write` with an error naming it.

#### Scenario: Moving a Transform from TS
- **WHEN** a TS system declares `write: [Transform]` and adds to `translation.y`
- **THEN** Bevy's own `Transform` of that entity changes and the rendered cube moves

#### Scenario: A Rust component without a layout
- **WHEN** a TS system declares `read: [GlobalTransform]` and no layout is registered for it
- **THEN** loading fails with an error naming `GlobalTransform`

### Requirement: Generated field accessors
For each component, the `bevy-ts` module SHALL give TS code a typed view object whose properties read and write the fields of one row in place (`mover.speed`, `t.translation.y`), plus the raw offsets and size for code that indexes the typed arrays itself. Every setter SHALL also store the run's change tick in the row's changed tick (see ts-systems). TS types for declared components SHALL be inferred from the `component()` call without a codegen step.

#### Scenario: Typed access in the editor
- **WHEN** a system's `run` writes `mover.sped = 2`
- **THEN** `tsc` reports that `sped` does not exist on the component's type
