# hot-reload Specification

## Purpose
The `bevy-ts dev` command, generations, what state survives a reload, schema migration, and failed reloads.

## Requirements

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
A reload SHALL keep every entity, component value, resource and parent link. It SHALL replace all systems, observers, plugins and component declarations with the new generation's. JS module-level state SHALL NOT survive; state that must survive belongs in components.

#### Scenario: The cube keeps its place
- **WHEN** a reload happens while the cube is at y = 1.4
- **THEN** the first frame of the new generation starts from y = 1.4

#### Scenario: A module-level counter
- **WHEN** a module keeps `let hits = 0` and increments it, then reloads
- **THEN** `hits` starts at 0 again in the new generation

### Requirement: Systems are identified by module and export name
Every system, observer and plugin SHALL be identified by `<module path>#<export name>` (for example `src/enemy.ts#chase`), assigned by the loader. That identity SHALL key ordering, per-system `Changed`/`Added` ticks, message cursors and `Local` storage across reloads. Renaming an export SHALL be reported as a removed and an added system; the new one starts with default `Local` values and an unread message cursor.

#### Scenario: Local survives an edit
- **WHEN** `waves` keeps `s: Local<{ wave: u32 }>` at 4 and its body is edited and reloads
- **THEN** `s.wave` is still 4 in the new generation

### Requirement: Startup systems run once per identity per world
A system in `Startup` SHALL run once the first time a generation registering its identity is applied, and SHALL NOT run again on later reloads. A `Startup` system with a new identity SHALL run when the generation that adds it is applied. When the game has not started (an engine under `--edit` that has not entered play or stepped, see edit-mode), a `Startup` system SHALL instead run when the game starts, and a generation applied before then SHALL run none. `bevy-ts/reload` with `restart: true` SHALL despawn every entity spawned by TS, forget which `Startup` systems ran, reset `Local` storage, remove every TS resource so the generation inserts it again with its `insertResource` value, clear every TS state and pending TS message, notify registered plugins (see plugin-api), and apply the generation as if first; in edit mode the game is then not started and `Startup` waits for play. A Stop (edit-mode) SHALL reset the world the same way, re-applying the live generation instead of compiling.

#### Scenario: Editing a startup's body
- **WHEN** the body of `Startup` system `scene` changes and reloads
- **THEN** it does not run again and no duplicate cube appears

#### Scenario: A save before the first Play
- **WHEN** an `--edit` engine has not played, `game.ts` gains `Startup` system `spawnWave` and reloads, then a client sets `play`
- **THEN** `spawnWave` runs once, at play, with the other `Startup` systems

#### Scenario: A restart resets the score
- **WHEN** `Score` was inserted with `value: 0`, a system has raised it to 7, and a client calls `bevy-ts/reload` with `restart: true`
- **THEN** `Score.value` is 0 after the restart

#### Scenario: A restart drops a pending message
- **WHEN** a system wrote a `Hit` message this frame and a client calls `bevy-ts/reload` with `restart: true`
- **THEN** no reader sees that `Hit` after the restart

### Requirement: Layout changes migrate by field name
When a component name is redeclared with a different layout, bevy-ts SHALL register a new `ComponentId` for it, copy each entity's value field by field where name and type match, fill new fields with their defaults (zero if none), drop removed fields, remove the old component from every entity, and report the migration. The old `ComponentId` SHALL become retired: hidden from the protocol and never matched by TS systems.

#### Scenario: Adding a field
- **WHEN** `Mover` gains `phase: f32 = 0` and reloads
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
A syntax error, a module that throws while evaluating, an invalid declaration or a failed migration SHALL leave the previous generation running untouched. When the failing generation is the first, the engine SHALL apply an empty generation instead: no systems, no scenes, an empty world. It SHALL keep serving the protocol, thumbnails and previews (operator ruling 2026-10-20: the studio keeps the project's engine running while the project is open). The error SHALL be printed in the terminal with its TS location and published on the protocol. The next successful save SHALL apply normally, as a first load when the running generation is the empty one.

#### Scenario: A syntax error
- **WHEN** the user saves `game.ts` with an unclosed brace
- **THEN** the cube keeps moving as before, the terminal shows the file and line, and `bevy-ts/reload+watch` reports `failed`

#### Scenario: Opening a project that does not load
- **WHEN** the studio starts the engine on a project whose `game.ts` throws while evaluating
- **THEN** the engine runs with an empty world, `bevy-ts/hello` and `asset_preview` answer, `reload+watch` reports `failed` with the location, and fixing the file applies the game as a first load

### Requirement: The debugger survives a reload
A soft reload SHALL load the new generation into the same isolate, so an attached CDP debugger stays connected and its breakpoints rebind by source location.

#### Scenario: A breakpoint across a reload
- **WHEN** a breakpoint is set in `bob` and `game.ts` is saved with an unrelated change
- **THEN** the debugger stays connected and the breakpoint hits in the new generation's `bob`

### Requirement: Installing a package reloads
`bevy-ts dev` SHALL watch, besides every loaded module, each `package.json` the resolver read in the last compile, and, for each path the resolver looked for and did not find, the nearest folder that exists, unless that folder is above the project root (an install in the project never creates one there). A change to any of them SHALL start a normal reload; no other reload path SHALL exist for packages.

#### Scenario: Install after a missing package
- **WHEN** a load failed because `lodash-es` is not installed, and the user runs `pnpm add lodash-es` in the project
- **THEN** the engine reloads on its own and the game loads with `lodash-es`

#### Scenario: Editing a package's exports
- **WHEN** a linked package's `package.json` changes its `exports` while `bevy-ts dev` runs
- **THEN** the engine reloads and resolves the package's imports anew
