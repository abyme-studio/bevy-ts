## MODIFIED Requirements

### Requirement: Components are plain classes whose fields the compile step reads
A component SHALL be declared as a plain class extending `Component`, its fields written as typed class fields, as a Rust struct is (operator ruling 2026-10-20):

```ts
export class Mover extends Component { speed: f32 = 1; amplitude: f32 = 2 }
export class Target extends Component { declare entity: Entity }
export class Tag extends Component { label: string = "crate"; scores: u32[] = [] }
```

The compile step SHALL read each field's name, type and default from syntax (script-runtime), with no schema object at runtime. A field's type SHALL be one of `f32`, `f64`, `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `bool`, `Vec2`, `Vec3`, `Vec4`, `Quat`, `Entity`, `Timer`, `Stopwatch` and a union of string literals (an enum); these SHALL be exported as TS types (`f32` is `number`). A component field MAY also be `string` (a heap field, Rust's `String`) or `T[]` with `T` one of `f32`, `f64`, `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `bool` or `string` (a heap field, Rust's `Vec<T>`). A field with an initialiser SHALL use it as its default. A field without one SHALL be written `declare name: type` and default to zero, `""` for a string and `[]` for a list. Objects, nested lists, lists of vectors or entities and other heap data SHALL fail the load with the file and line.

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

#### Scenario: A timer field
- **WHEN** a component declares `cooldown = Timer.fromSeconds(2, "Repeating")`
- **THEN** it loads, and a new entity's `cooldown` has `duration` 2, `mode` `"Repeating"` and `elapsed` 0

### Requirement: Native layout rules
A TS component's layout SHALL follow C struct rules: fields in declaration order, each at an offset aligned to its own alignment, total size rounded up to the largest field alignment. `Vec2`/`Vec3`/`Vec4`/`Quat` are `f32` arrays aligned to 4; `bool` is one byte holding 0 or 1; `Entity` is a `u64` holding `Entity::to_bits`, aligned to 8; a string-literal union is the smallest unsigned integer holding its variants. A `Timer` is 24 bytes aligned to 8: `elapsed` and `duration` (`f64` seconds), `times_finished_this_tick` (`u32`), `mode` (`u8`, `Once` 0, `Repeating` 1), `paused` and `finished` (`bool`). A `Stopwatch` is 16 bytes aligned to 8: `elapsed` (`f64` seconds) and `paused` (`bool`). Rust SHALL compute the layout and hand the offsets back to JS; JS SHALL NOT compute its own.

#### Scenario: Offsets of a mixed component
- **WHEN** a component declares `{ alive: bool, hp: f32, pos: vec3 }`
- **THEN** its layout is size 20, align 4, with `alive` at 0, `hp` at 4 and `pos` at 8

#### Scenario: Offsets around a timer
- **WHEN** a component declares `{ hp: f32, cooldown: Timer }`
- **THEN** its layout is size 32, align 8, with `hp` at 0 and `cooldown` at 8

## ADDED Requirements

### Requirement: Timer and Stopwatch are Bevy's, ported to TS
`Timer`, `Stopwatch` and `TimerMode` (`"Once" | "Repeating"`) SHALL be field types of TS components, resources and `Local`s, ported from Bevy 0.20's `bevy_time` with every `Duration` as seconds. `Timer.new(duration, mode)`, `Timer.fromSeconds(seconds, mode)` and `Stopwatch.new()` SHALL make a value for an initialiser or `C.of({...})`.

A field's view SHALL have Bevy's getters as snake_case properties: on a timer `elapsed`, `elapsed_secs`, `duration`, `mode`, `is_paused`, `is_finished`, `just_finished`, `times_finished_this_tick`, `remaining`, `remaining_secs`, `fraction` and `fraction_remaining`; on a stopwatch `elapsed`, `elapsed_secs` and `is_paused`. A written field (`Mut<C>`, `ResMut<R>`, `Local`) SHALL add Bevy's methods in camelCase: on a timer `tick(seconds)` (returning the view), `pause`, `unpause`, `reset`, `finish`, `almostFinish`, `setElapsed`, `setDuration` and `setMode`; on a stopwatch `tick(seconds)`, `pause`, `unpause`, `reset` and `setElapsed`. Each method SHALL follow Bevy's semantics, allocate nothing and store the row's change tick once per call. A read field SHALL be typed without the methods.

Rust SHALL read and write a TS `Timer` or `Stopwatch` field through the protocol, the inspector and `.bsn` files as a struct of the fields named in its layout, `mode` by variant name. A Rust component holding Bevy's own `Timer` is unchanged by this requirement.

#### Scenario: A repeating timer finishes like Bevy's
- **WHEN** a TS system ticks a `Timer.fromSeconds(0.5, "Repeating")` field by 0.2, 0.2, 0.2 and 1.0 s, and Rust ticks Bevy's `Timer::from_seconds(0.5, TimerMode::Repeating)` by the same
- **THEN** after each tick `just_finished`, `times_finished_this_tick` and `elapsed` match Bevy's (`elapsed` within 1e-9 s), ending with 2 finishes on the last tick

#### Scenario: A once timer stays finished
- **WHEN** a `"Once"` timer of 1 s is ticked by 1.5 s, then by 0.1 s
- **THEN** the first tick has `just_finished` true and `elapsed` 1, the second `just_finished` false and `is_finished` true

#### Scenario: A paused stopwatch
- **WHEN** a stopwatch is ticked by 1 s, paused, ticked by 1 s, unpaused and ticked by 1 s
- **THEN** its `elapsed` is 2

#### Scenario: A read timer has no methods
- **WHEN** a system with `Query<[Cooldown]>` calls `c.cooldown.tick(dt)`
- **THEN** the type check fails, saying `tick` does not exist on the read-only view
