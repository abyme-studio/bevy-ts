## ADDED Requirements

### Requirement: The dev command runs a precompiled engine
`bevy-ts dev <entry.ts>` SHALL start the precompiled engine binary with a window, compile the entry and its relative imports, load them, and watch every loaded file. No Rust compilation SHALL happen. `--remote-port` SHALL set the protocol port (default 15702) and `--inspect[=port]` SHALL enable the CDP server.

#### Scenario: First launch
- **WHEN** a user with no Rust toolchain runs `bevy-ts dev src/game.ts`
- **THEN** a window opens running the game

### Requirement: Reloads are generations applied at a frame boundary
A saved file, or a `bevy-ts/reload` request, SHALL recompile the module graph on a background thread and produce a new generation numbered one above the last. The engine SHALL apply a generation at the start of a frame, before any bevy-ts stage, and SHALL never run systems from two generations in one frame.

#### Scenario: Saving a file
- **WHEN** the user saves `game.ts` with a new constant in `bob`
- **THEN** within 200 ms on a recent laptop the cube moves with the new constant, and the generation number goes up by one

### Requirement: ECS state survives a reload
A reload SHALL keep every entity, component value, resource and parent link. It SHALL replace all systems, startups and component declarations with the new generation's. JS module-level state SHALL NOT survive; state that must survive belongs in components.

#### Scenario: The cube keeps its place
- **WHEN** a reload happens while the cube is at y = 1.4
- **THEN** the first frame of the new generation starts from y = 1.4

#### Scenario: A module-level counter
- **WHEN** a module keeps `let hits = 0` and increments it, then reloads
- **THEN** `hits` starts at 0 again in the new generation

### Requirement: Startups run once per name per world
A startup SHALL run once the first time a generation declaring its name is applied, and SHALL NOT run again on later reloads. A startup with a new name SHALL run when the generation that adds it is applied. `bevy-ts/reload` with `restart: true` SHALL despawn every entity spawned by TS, forget which startups ran, notify registered plugins (see plugin-api), and apply the generation as if first.

#### Scenario: Editing a startup's body
- **WHEN** the body of startup `scene` changes and reloads
- **THEN** it does not run again and no duplicate cube appears

### Requirement: Layout changes migrate by field name
When a component name is redeclared with a different layout, bevy-ts SHALL register a new `ComponentId` for it, copy each entity's value field by field where name and type match, fill new fields with their defaults (zero if none), drop removed fields, remove the old component from every entity, and report the migration. The old `ComponentId` SHALL become retired: hidden from the protocol and never matched by TS systems.

#### Scenario: Adding a field
- **WHEN** `game::Mover` gains `phase: f32.default(0)` and reloads
- **THEN** every cube keeps its `speed` and `amplitude`, has `phase = 0`, and the log says `game::Mover` migrated on N entities

#### Scenario: Changing a field's type
- **WHEN** `speed` changes from `f32` to `i32`
- **THEN** `speed` takes its default on every entity and the migration report names the field as reset

### Requirement: Undeclared components are kept
A component that a new generation no longer declares SHALL stay on its entities with its data, marked undeclared in the protocol's schema. Redeclaring it later with the same layout SHALL make it live again with its data.

#### Scenario: Comment out and back
- **WHEN** `game::Mover`'s declaration is commented out, reloaded, restored and reloaded
- **THEN** every cube still has its last `Mover` values

### Requirement: A failed reload keeps the running generation
A syntax error, a module that throws while evaluating, an invalid declaration or a failed migration SHALL leave the previous generation running untouched. The error SHALL be printed in the terminal with its TS location and published on the protocol. The next successful save SHALL apply normally.

#### Scenario: A syntax error
- **WHEN** the user saves `game.ts` with an unclosed brace
- **THEN** the cube keeps moving as before, the terminal shows the file and line, and `bevy-ts/reload+watch` reports `failed`

### Requirement: The debugger survives a reload
A soft reload SHALL load the new generation into the same isolate, so an attached CDP debugger stays connected and its breakpoints rebind by source location.

#### Scenario: A breakpoint across a reload
- **WHEN** a breakpoint is set in `bob` and `game.ts` is saved with an unrelated change
- **THEN** the debugger stays connected and the breakpoint hits in the new generation's `bob`
