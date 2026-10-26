## MODIFIED Requirements

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

### Requirement: Heap fields in TS systems
A TS system SHALL read a heap field through its row accessor (`tag.label`, `tag.scores`), the value fetched from Rust on the first read and kept until the row's changed tick moves. Through a read-only accessor a list SHALL read as a frozen array typed `readonly T[]`, and `tag.scores.push(1)` throws a `TypeError`. Through a writable accessor (`Mut<C>`, `ResMut<R>`, `Local`) a list SHALL read as a mutable array typed `T[]`, edited in place with JS's array API (`push`, `pop`, `tag.scores[i] = x`, `length = n`, item fields of a struct or vector item); the edits SHALL be applied in Rust at the end of the system's run, before any later system, as item edits (set, append, truncate) when the change is one of those and as a whole write otherwise, with the change tick stored once. A list read through a writable accessor and left unchanged SHALL write nothing and mark nothing changed. A heap read from Rust in the same run of a row with pending edits SHALL see them. A whole assignment (`tag.label = "x"`, `tag.scores = [1, 2]`) SHALL apply at once, store the run's change tick and be seen by Rust and by later reads the same frame. `q.tables` SHALL offer no raw arrays for a component with a heap field. A query over a component with no heap field SHALL make no host call for it.

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

#### Scenario: A list read and left alone
- **WHEN** a system with `Query<[Mut<Tag>]>` reads `tag.scores.length` on every row and edits none
- **THEN** no row is marked changed

## ADDED Requirements

### Requirement: Heap fields wherever a layout is declared
A `string` or `T[]` field SHALL be allowed, with the same types, defaults and storage as on a component, in a TS resource, a `Local`, a message and an event. Each value SHALL be built in Rust, owned by the store that holds it (the resource entity, the system's memory, the message queue, the pending event) and freed exactly once: when it is replaced, when a message expires on Bevy's two-frame rule, when the last observer of an event has run, on a restart, and when the world drops. A reload that keeps a heap field of a `Local` or resource SHALL move its value. A message or event's heap field SHALL be read-only to its readers and observers, as Bevy's `MessageReader` and `On` give `&M`. Scene props keep their own `string` support.

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
For a component with a heap field, the value an `On<Remove<C>>`, `On<Despawn<C>>` or `On<Discard<C>>` observer gets SHALL be a copy taken when the event fires, owning its own heap values, so its heap fields read as they were then, even once the component or entity is gone. A component with no heap field SHALL keep today's byte copy.

#### Scenario: A name read after despawn
- **WHEN** an entity with `Tag { label: "door" }` is despawned and an observer on `On<Despawn<Tag>>` reads `label`
- **THEN** it reads `"door"`

## REMOVED Requirements

### Requirement: Heap fields are allowed on components only
**Reason**: heap fields are now allowed on resources, `Local`, messages and events.
**Migration**: none; declarations that failed now load (see "Heap fields wherever a layout is declared").
