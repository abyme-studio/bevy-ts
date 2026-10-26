## MODIFIED Requirements

### Requirement: Components are plain classes whose fields the compile step reads
A component SHALL be declared as a plain class extending `Component`, its fields written as typed class fields, as a Rust struct is (operator ruling 2026-10-20):

```ts
export class Mover extends Component { speed: f32 = 1; amplitude: f32 = 2 }
export class Target extends Component { declare entity: Entity }
export class Tag extends Component { label: string = "crate"; scores: u32[] = [] }
```

The compile step SHALL read each field's name, type and default from syntax (script-runtime), with no schema object at runtime. A field's type SHALL be one of `f32`, `f64`, `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `bool`, `Vec2`, `Vec3`, `Vec4`, `Quat`, `Entity` and a union of string literals (an enum); these SHALL be exported as TS types (`f32` is `number`). A component field MAY also be `string` (a heap field, Rust's `String`) or `T[]` with `T` one of `f32`, `f64`, `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `bool` or `string` (a heap field, Rust's `Vec<T>`). A field with an initialiser SHALL use it as its default. A field without one SHALL be written `declare name: type` and default to zero, `""` for a string and `[]` for a list. Objects, nested lists, lists of vectors or entities and other heap data SHALL fail the load with the file and line.

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

#### Scenario: A refused heap type
- **WHEN** a component declares `points: Vec3[]` or `meta: { a: f32 }`
- **THEN** loading fails with an error naming the class, the field, the file and the line

#### Scenario: A pinned name survives a rename
- **WHEN** `class Shield extends Component { static path = "game::Shield"; strength: f32 = 0 }` is renamed to `Barrier` and reloads
- **THEN** it keeps the `ComponentId` of `game::Shield`, and every entity keeps its value

### Requirement: Registration as dynamic ECS components
Each TS component SHALL be registered with `World::register_component_with_descriptor` and `ComponentDescriptor::new_with_layout`: table storage, mutable, summary tick on. A component with no heap field SHALL have no drop function and copy its bytes when cloned. A component with a heap field SHALL have a drop function that frees each heap field and a clone function that deep-copies each one, so a clone owns its own values. Its name in Bevy SHALL be the declared name. It SHALL be queryable by Rust through its `ComponentId` and usable in `QueryBuilder`, `insert_by_id` and `get_by_id`.

#### Scenario: A Rust system reads a TS component
- **WHEN** a Rust system builds a query with `QueryBuilder::ref_id` on `game::Mover`'s `ComponentId`
- **THEN** it iterates every entity carrying `game::Mover` and reads its bytes in the declared layout

#### Scenario: Every heap value is freed once
- **WHEN** entities with a `string` field are spawned, cloned with `cloneAndSpawn`, migrated by a reload, despawned, and the world is dropped
- **THEN** every string built is freed exactly once, and each clone's string is its own

## ADDED Requirements

### Requirement: Heap fields live in the component's bytes
A heap field's value SHALL be stored in the component's own bytes as Rust's `String` or `Vec<T>`, as a Bevy component stores it. A component with a heap field SHALL start with a header Rust writes and JS never sees; the layout Rust hands JS SHALL give every field's offset after it. A component with no heap field SHALL keep exactly today's layout. Every path that builds a component value (commands, TSX, `.bsn` levels, the plugin API, saves, the protocol) SHALL build its heap values in Rust; bytes from JS SHALL never be read at a heap field's offset.

#### Scenario: A plain component's layout is unchanged
- **WHEN** a component declares `{ alive: bool, hp: f32, pos: vec3 }`
- **THEN** its layout is size 20, align 4, with `alive` at 0, `hp` at 4 and `pos` at 8

#### Scenario: A TSX prop and a level value
- **WHEN** a TSX scene places `<Tag label="door" />` and a `.bsn` level places `Tag { label: "gate" }`
- **THEN** each entity's `label` reads back the string it was given

### Requirement: Heap fields in TS systems
A TS system SHALL read a heap field through its row accessor (`tag.label`, `tag.scores`), the value fetched from Rust on the first read and kept until the row's changed tick moves. A list SHALL read as a frozen array typed `readonly T[]`. A `Mut<C>` SHALL assign a whole heap field (`tag.label = "x"`, `tag.scores = [1, 2]`); the write SHALL apply at once, store the run's change tick and be seen by Rust and by later reads the same frame. In-place list edits SHALL not be offered: `tag.scores.push(1)` throws. `q.tables` SHALL offer no raw arrays for a component with a heap field. A query over a component with no heap field SHALL make no host call for it.

#### Scenario: A write is seen at once
- **WHEN** a system with `Query<[Mut<Tag>]>` sets `tag.label = "open"` and then reads `tag.label`
- **THEN** it reads `"open"`, a Rust system after it sees `"open"`, and `Changed<Tag>` matches the row

#### Scenario: An untouched row makes no host call
- **WHEN** a system reads `tag.label` on 1,000 rows on two frames and nothing writes `Tag` in between
- **THEN** the second frame makes no host call for `Tag`

#### Scenario: A push on a read list
- **WHEN** a system calls `tag.scores.push(3)`
- **THEN** it throws a `TypeError` and the component is unchanged

### Requirement: Reload moves heap fields
A reload that changes a component's layout SHALL keep a heap field of the same name and type by moving its value into the new component, not copying it. A heap field whose type changed SHALL be reset to its default and its old value freed.

#### Scenario: A field is added beside a string
- **WHEN** `Tag` holds `label: "door"` and a reload adds `weight: f32 = 1`
- **THEN** the entity has `label` `"door"` and `weight` 1, and the old value is not freed while the new one still holds it

### Requirement: Heap fields are allowed on components only
A `string` or `T[]` field in a `Local`, a TS resource, a message or an event SHALL fail the load with the file, the line and a message that heap fields are allowed on components only. Scene props keep their own `string` support.

#### Scenario: A string in a resource
- **WHEN** a TS resource declares `title: string = "x"`
- **THEN** loading fails naming the resource, the field, the file and the line, and saying heap fields are allowed on components only
