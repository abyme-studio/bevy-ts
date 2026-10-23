# ts-components Specification

## Purpose
How TS declares components (class-based, with native layouts), registers them as real ECS components, and reads and writes their fields in place.

## Requirements

### Requirement: Components are plain classes whose fields the compile step reads
A component SHALL be declared as a plain class extending `Component`, its fields written as typed class fields, as a Rust struct is (operator ruling 2026-10-20):

```ts
export class Mover extends Component { speed: f32 = 1; amplitude: f32 = 2 }
export class Target extends Component { declare entity: Entity }
```

The compile step SHALL read each field's name, type and default from syntax (script-runtime), with no schema object at runtime. A field's type SHALL be one of `f32`, `f64`, `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `bool`, `Vec2`, `Vec3`, `Vec4`, `Quat`, `Entity` and a union of string literals (an enum); these SHALL be exported as TS types (`f32` is `number`). A field with an initialiser SHALL use it as its default. A field without one SHALL be written `declare name: type` and default to zero. `string`, arrays, objects and other heap data SHALL fail the load with the file and line.

The class SHALL be the component's handle in queries (`Query<[Mover]>`), its TSX tag (`<Mover speed={2} />`) and, through `Mover.of({ speed: 2 })`, its constructor for commands. Its instance type SHALL be the component's data. A class with no fields is a marker. A class extending `SceneComponent` is a component whose `static scene(props)` builds a scene spawned with it, as Bevy's `#[derive(SceneComponent)]` (tsx, levels).

The component's name in Bevy, the protocol and `.bsn` files SHALL default to a Rust-style path: the module's path under the source root with `/` as `::` and the extension dropped, then the class or export: `src/enemies.tsx` exporting `Grunt` is `enemies::Grunt`, `src/units/tank.ts` exporting `Tank` is `units::tank::Tank`, as a Rust type path is built from its module (`src/game.ts` exporting `Mover` is `game::Mover`). Two classes with one derived path SHALL fail the load naming both. `static path = "game::Mover"` SHALL pin it, so moving or renaming the class keeps its identity.

#### Scenario: A plain numeric component
- **WHEN** `game.ts` exports `class Mover extends Component { speed: f32 = 1; amplitude: f32 = 2 }`
- **THEN** a component named `game::Mover` with two `f32` fields, defaulting to 1 and 2, exists in the world after the module loads

#### Scenario: A string field
- **WHEN** a component declares `name: string = "x"`
- **THEN** loading fails with an error naming the class, the field, the file and the line

#### Scenario: A pinned name survives a rename
- **WHEN** `class Shield extends Component { static path = "game::Shield"; strength: f32 = 0 }` is renamed to `Barrier` and reloads
- **THEN** it keeps the `ComponentId` of `game::Shield`, and every entity keeps its value

### Requirement: Native layout rules
A TS component's layout SHALL follow C struct rules: fields in declaration order, each at an offset aligned to its own alignment, total size rounded up to the largest field alignment. `Vec2`/`Vec3`/`Vec4`/`Quat` are `f32` arrays aligned to 4; `bool` is one byte holding 0 or 1; `Entity` is a `u64` holding `Entity::to_bits`, aligned to 8; a string-literal union is the smallest unsigned integer holding its variants. Rust SHALL compute the layout and hand the offsets back to JS; JS SHALL NOT compute its own.

#### Scenario: Offsets of a mixed component
- **WHEN** a component declares `{ alive: bool, hp: f32, pos: vec3 }`
- **THEN** its layout is size 20, align 4, with `alive` at 0, `hp` at 4 and `pos` at 8

### Requirement: Registration as dynamic ECS components
Each TS component SHALL be registered with `World::register_component_with_descriptor` and `ComponentDescriptor::new_with_layout`: table storage, no drop function, mutable, summary tick on, default clone behaviour. Its name in Bevy SHALL be the declared name. It SHALL be queryable by Rust through its `ComponentId` and usable in `QueryBuilder`, `insert_by_id` and `get_by_id`.

#### Scenario: A Rust system reads a TS component
- **WHEN** a Rust system builds a query with `QueryBuilder::ref_id` on `game::Mover`'s `ComponentId`
- **THEN** it iterates every entity carrying `game::Mover` and reads its bytes in the declared layout

### Requirement: A component name keeps its identity
Declaring the same name (default or pinned) with the same layout in a later generation SHALL reuse the existing `ComponentId`. Two declarations of one name in one generation SHALL fail the load. Moving or renaming a class without a pinned `path` SHALL be reported as a new component; its old data stays with the old name (hot-reload, undeclared components are kept).

#### Scenario: A reload without schema change
- **WHEN** `game.ts` is edited without touching `Mover`'s fields and reloads
- **THEN** `Mover` keeps its `ComponentId` and every entity keeps its value

### Requirement: Rust components cross only with a declared native layout
A Rust component SHALL be usable as query data in a TS system only if a native layout for it is registered in Rust: from reflection, for its plain-data leaves (offsets from a value's field references, bounds-checked; other leaves are holes TS never sees or writes; operator ruling 2026-10-20, native decision 5), or from `core::mem::offset_of!` for a `NativeLayout` derive, with the type's real size and alignment either way. `Transform` (`translation: vec3`, `rotation: quat`, `scale: vec3`) SHALL take the reflection path like any other type and be exposed to TS as `Transform` from `bevy-ts/bevy`. A Rust component with no native layout SHALL be rejected as query data with an error naming it; it MAY be used in `With`/`Without` filters.

#### Scenario: Moving a Transform from TS
- **WHEN** a TS system queries `Mut<Transform>` and adds to `translation.y`
- **THEN** Bevy's own `Transform` of that entity changes and the rendered cube moves

#### Scenario: A Rust component without a layout
- **WHEN** a TS system queries `GlobalTransform` and no layout is registered for it
- **THEN** loading fails with an error naming `GlobalTransform`

### Requirement: Generated field accessors
For each component, the `bevy-ts` module SHALL give TS code a typed view object whose properties read and write the fields of one row in place (`mover.speed`, `t.translation.y`), plus the raw offsets and size for code that indexes the typed arrays itself. Every setter SHALL also store the run's change tick in the row's changed tick (see ts-systems). TS types for declared components SHALL be the classes themselves, with no codegen step. A component read by a query SHALL be typed read-only: its fields `readonly`, its vector fields `ReadonlyVec3`/`ReadonlyQuat` without mutating methods. A written component (`Mut<C>`) SHALL type its scalar fields as assignable and its vector fields as views that are not reassignable and are mutated in place with glam-style methods that write `this` and allocate nothing (`set`, `copy`, `add`, `sub`, `scale`, `addScaled`, `normalize`, `lerp`, `subVectors`, `rotate`, ...). `vec3(x, y, z)` and `quat()` SHALL make scratch values with the same methods. A vector view SHALL read its row from its parent accessor, not keep a copy updated by a setter on every row move (operator ruling 2026-10-20: nested views measured 3.5× raw arrays at 1M rows when re-positioned per row, `bench/dx/results/iter-forms.md`). A vector method that writes several components (`addScaled`, `normalize`, `lerp`, ...) SHALL store the row's change tick once per call, not once per component; a plain field setter keeps its one store, the form measured cheapest on V8 (`docs/research/change-detection.md`). An accessor SHALL keep its own state (its row, the run's tick, the row's tick column, its typed views and its nested views) under keys no field can name, so a field may take any name, and SHALL NOT refuse or rename a field for its name.

#### Scenario: A field named like the accessor's state
- **WHEN** a component, a resource, a `Local`, a message or an event declares fields such as `t: Vec3`, `r: f32`, `k: u32`, `f32: f32` or `present: f32`, and a system writes them
- **THEN** each field reads back the value written, and a written component's row is marked changed

#### Scenario: Typed access in the editor
- **WHEN** a system's `run` writes `mover.sped = 2`
- **THEN** `tsc` reports that `sped` does not exist on the component's type

#### Scenario: Writing a read component
- **WHEN** a system with `q: Query<[Mover]>` writes `mover.speed = 2`
- **THEN** `tsc` reports that `speed` is read-only

#### Scenario: A vector method marks once
- **WHEN** a system calls `t.translation.addScaled(dir, s)` on a row
- **THEN** all three components are written and the row's changed tick is stored once
