# ts-components Specification

## Purpose
How TS declares components (class-based, with native layouts), registers them as real ECS components, and reads and writes their fields in place.

## Requirements

### Requirement: Components are plain classes whose fields the compile step reads
A component SHALL be declared as a plain class extending `Component`, its fields written as typed class fields, as a Rust struct is (operator ruling 2026-10-20):

```ts
export class Mover extends Component { speed: f32 = 1; amplitude: f32 = 2 }
export class Target extends Component { declare entity: Entity }
export class Tag extends Component { label: string = "crate"; scores: u32[] = [] }
type Point = { x: f32; y: f32 }
export class Path extends Component { declare points: Point[]; declare stops: Entity[] }
```

The compile step SHALL read each field's name, type and default from syntax (script-runtime), with no schema object at runtime. A field's type SHALL be one of `f32`, `f64`, `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `bool`, `Vec2`, `Vec3`, `Vec4`, `Quat`, `Entity` and a union of string literals (an enum); these SHALL be exported as TS types (`f32` is `number`). A field MAY also be `string` (a heap field, Rust's `String`) or `T[]` (a heap field, Rust's `Vec<T>`) with `T` a number kind, `bool`, `string`, `Vec2`, `Vec3`, `Vec4`, `Quat`, `Entity`, a string-literal union, or an object type (a literal, an alias or an interface) whose members are all of the plain types above (a struct item). A field with an initialiser SHALL use it as its default. A field without one SHALL be written `declare name: type` and default to zero, `""` for a string and `[]` for a list. Nested lists, a struct item with a `string` or list member, an object field outside a list and other heap data SHALL fail the load with the file and line.

The class SHALL be the component's handle in queries (`Query<[Mover]>`), its TSX tag (`<Mover speed={2} />`) and, through `Mover.of({ speed: 2 })`, its constructor for commands. Its instance type SHALL be the component's data. A class with no fields is a marker. A class extending `SceneComponent` is a component whose `static scene(props)` builds a scene spawned with it, as Bevy's `#[derive(SceneComponent)]` (tsx, levels).

The component's name in Bevy, the protocol and `.bsn` files SHALL default to a Rust-style path: the module's path under the source root with `/` as `::` and the extension dropped, then the class or export: `src/enemies.tsx` exporting `Grunt` is `enemies::Grunt`, `src/units/tank.ts` exporting `Tank` is `units::tank::Tank`, as a Rust type path is built from its module (`src/game.ts` exporting `Mover` is `game::Mover`). Two classes with one derived path SHALL fail the load naming both. `static path = "game::Mover"` SHALL pin it, so moving or renaming the class keeps its identity.

#### Scenario: A plain numeric component
- **WHEN** `game.ts` exports `class Mover extends Component { speed: f32 = 1; amplitude: f32 = 2 }`
- **THEN** a component named `game::Mover` with two `f32` fields, defaulting to 1 and 2, exists in the world after the module loads

#### Scenario: A string field
- **WHEN** a component declares `name: string = "x"` and `Tag.of({})` is spawned
- **THEN** the entity's `name` reads `"x"` in a TS system, in Rust through the component's bytes and over the protocol

#### Scenario: A list field
- **WHEN** a component declares `scores: u32[] = [1, 2]` and an entity is spawned with `scores: [7]`
- **THEN** a TS system reads `[7]`, and another entity spawned without it reads `[1, 2]`

#### Scenario: Lists of vectors, entities and structs
- **WHEN** a component declares `points: Vec3[]`, `stops: Entity[]` and `marks: Point[]` with `type Point = { x: f32; y: f32 }`, and an entity is spawned with two of each
- **THEN** a TS system reads each item back, and a Rust system reads `points` as a `Vec<Vec3>` and `marks` as a `Vec` of a `#[repr(C)]` struct `{ x: f32, y: f32 }`

#### Scenario: A refused heap type
- **WHEN** a component declares `grid: f32[][]`, `meta: { a: f32 }` or `items: { name: string }[]`
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
Each TS component SHALL be registered with `World::register_component_with_descriptor` and `ComponentDescriptor::new_with_layout`: table storage, mutable, summary tick on. A component with no heap field SHALL have no drop function and copy its bytes when cloned. A component with a heap field SHALL have a drop function that frees each heap field and a clone function that deep-copies each one, so a clone owns its own values. Its name in Bevy SHALL be the declared name. It SHALL be queryable by Rust through its `ComponentId` and usable in `QueryBuilder`, `insert_by_id` and `get_by_id`.

#### Scenario: A Rust system reads a TS component
- **WHEN** a Rust system builds a query with `QueryBuilder::ref_id` on `game::Mover`'s `ComponentId`
- **THEN** it iterates every entity carrying `game::Mover` and reads its bytes in the declared layout

#### Scenario: Every heap value is freed once
- **WHEN** entities with a `string` field are spawned, cloned with `cloneAndSpawn`, migrated by a reload, despawned, and the world is dropped
- **THEN** every string built is freed exactly once, and each clone's string is its own

#### Scenario: cloneAndSpawn copies a TS component
- **WHEN** a system calls `commands.entity(e).cloneAndSpawn()` on an entity with `Health.of({ hp: 7 })`
- **THEN** the new entity has `Health` with the same bytes, `hp` 7, and changing one leaves the other as it was

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

### Requirement: Heap fields live in the component's bytes
A heap field's value SHALL be stored in the component's own bytes as Rust's `String` or `Vec<T>`, as a Bevy component stores it. A component with a heap field SHALL start with a header Rust writes and JS never sees; the layout Rust hands JS SHALL give every field's offset after it. A component with no heap field SHALL keep exactly today's layout. Every path that builds a component value (commands, TSX, `.bsn` levels, the plugin API, saves, the protocol) SHALL build its heap values in Rust; bytes from JS SHALL never be read at a heap field's offset.

#### Scenario: A plain component's layout is unchanged
- **WHEN** a component declares `{ alive: bool, hp: f32, pos: vec3 }`
- **THEN** its layout is size 20, align 4, with `alive` at 0, `hp` at 4 and `pos` at 8

#### Scenario: A TSX prop and a level value
- **WHEN** a TSX scene places `<Tag label="door" />` and a `.bsn` level places `Tag { label: "gate" }`
- **THEN** each entity's `label` reads back the string it was given

### Requirement: Heap fields in TS systems
A TS system SHALL read a heap field through its row accessor (`tag.label`, `tag.scores`), the value fetched from Rust on the first read and kept until the row's changed tick moves. Through a read-only accessor a list SHALL read as a frozen array typed `readonly T[]`, and `tag.scores.push(1)` throws a `TypeError`. Through a writable accessor (`Mut<C>`, `ResMut<R>`, `Local`) a list SHALL read as a mutable array typed `T[]`, edited in place with JS's array API (`push`, `pop`, `tag.scores[i] = x`, `length = n`, item fields of a struct or vector item); the edits SHALL be applied in Rust at the end of the system's run, before any later system, as item edits (set, append, truncate) when the change is one of those and as a whole write otherwise, with the change tick stored once. A list read through a writable accessor and left unchanged SHALL write nothing and mark nothing changed. Items SHALL be compared with `Object.is`. A list with a hole or an `undefined` item at the end of the run (a grown `length`, `a[i] = x` past the end) SHALL fail the run with a `TypeError` naming the field and the index, and write nothing for that list. A heap read from Rust in the same run of a row with pending edits SHALL see them. A whole assignment (`tag.label = "x"`, `tag.scores = [1, 2]`) SHALL apply at once, store the run's change tick and be seen by Rust and by later reads the same frame; it SHALL replace the array and its snapshot, so edits before it are dropped and edits after it are taken against it. `q.tables` SHALL offer no raw arrays for a component with a heap field. A query over a component with no heap field SHALL make no host call for it.

#### Scenario: A write is seen at once
- **WHEN** a system with `Query<[Mut<Tag>]>` sets `tag.label = "open"` and then reads `tag.label`
- **THEN** it reads `"open"`, a Rust system after it sees `"open"`, and `Changed<Tag>` matches the row

#### Scenario: An untouched row makes no host call
- **WHEN** a system reads `tag.label` on 1,000 rows on two frames and nothing writes `Tag` in between
- **THEN** the second frame makes no host call for `Tag`

#### Scenario: A push on a read list
- **WHEN** a system with `Query<[Tag]>` calls `tag.scores.push(3)`
- **THEN** it throws a `TypeError` and the component is unchanged

#### Scenario: A push in place
- **WHEN** a system with `Query<[Mut<Tag>]>` calls `tag.scores.push(4)` on a row holding `[1, 2, 3]`, and sets `tag.scores[0] = 9`
- **THEN** a Rust system after it reads `[9, 2, 3, 4]`, the write reached Rust as one set and one append, and `Changed<Tag>` matches the row

#### Scenario: An item past the end
- **WHEN** a system with `Query<[Mut<Tag>]>` sets `tag.scores[10] = 1` on a list of 3
- **THEN** the run fails with a `TypeError` naming `scores` and index 3, and the list is still its 3 items

#### Scenario: An assignment after a push
- **WHEN** a system calls `tag.scores.push(4)` then assigns `tag.scores = [7]`, then pushes 8
- **THEN** a Rust system after it reads `[7, 8]`

#### Scenario: A list read and left alone
- **WHEN** a system with `Query<[Mut<Tag>]>` reads `tag.scores.length` on every row and edits none
- **THEN** no row is marked changed

### Requirement: Reload moves heap fields
A reload that changes a component's layout SHALL keep a heap field of the same name and type by moving its value into the new component, not copying it. A heap field whose type changed SHALL be reset to its default and its old value freed.

#### Scenario: A field is added beside a string
- **WHEN** `Tag` holds `label: "door"` and a reload adds `weight: f32 = 1`
- **THEN** the entity has `label` `"door"` and `weight` 1, and the old value is not freed while the new one still holds it

### Requirement: Heap fields wherever a layout is declared
A `string` or `T[]` field SHALL be allowed, with the same types, defaults and storage as on a component, in a TS resource, a `Local`, a message and an event. Each value SHALL be built in Rust, owned by the store that holds it (the resource entity, the system's memory, the message queue, the pending event) and freed exactly once: when it is replaced, when a message expires on Bevy's two-frame rule, when the last observer of an event has run, on a restart, and when the world drops. A reload that keeps a heap field of a `Local` or resource SHALL move its value. A message or event's heap field SHALL be read-only to its readers and observers, as Bevy's `MessageReader` and `On` give `&M`. A `runSystemWith` input (`In<E>`) of an event with a heap field SHALL throw a `TypeError` naming the event, until a later change carries it. Scene props keep their own `string` support.

#### Scenario: A string in a resource
- **WHEN** a TS resource declares `title: string = "x"`, a system with `ResMut<Title>` sets `title.title = "y"` and a later system with `Res<Title>` reads it
- **THEN** the later system reads `"y"`, and a second read of the unchanged resource makes no host call

#### Scenario: A list in a Local
- **WHEN** a system takes `Local<{ seen: Entity[] }>`, pushes an entity each run, and its module reloads with another field added
- **THEN** each run sees the entities of the runs before, across the reload

#### Scenario: A message with a string
- **WHEN** a system writes `Chat.of({ text: "hi" })` and two later systems read `Chat`
- **THEN** each reads `"hi"` once, and the string is freed once, when the message expires

#### Scenario: An event with a list
- **WHEN** `commands.trigger(Hit.of({ entity: e, tags: ["fire"] }))` bubbles to two entities, each with an observer
- **THEN** both observers read `["fire"]`, and the list is freed once after the last one has run

### Requirement: Lifecycle values carry a copy of the component
For a component with a heap field, the value an `On<Remove<C>>`, `On<Despawn<C>>` or `On<Discard<C>>` observer gets SHALL be a copy taken when the event fires, owning its own heap values, so its heap fields read as they were then, even once the component or entity is gone. Its heap fields SHALL be read at the observer place, never as a resource. A component with no heap field SHALL keep today's byte copy.

This is a behaviour change: under ts-component-heap-fields such a read was routed as a resource read (`reflect_codec.rs`, a negative entity) and always threw.

#### Scenario: A name read after despawn
- **WHEN** an entity with `Tag { label: "door" }` is despawned and an observer on `On<Despawn<Tag>>` reads `label`
- **THEN** it reads `"door"`
