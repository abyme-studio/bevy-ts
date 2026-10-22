# levels Specification

## Purpose
Levels say where things are. A level is a `.bsn` file that places TS scene components by name, overrides their fields per instance, includes other levels and adds plain entities; it loads as a root of the `tsx` reconciler, so a level save behaves like a TSX save, and every placed entity knows the level frames that own it.

## Requirements

### Requirement: A level is a reconciled root
`app.level(path)` SHALL register a `.bsn` file under the asset root as a root named `level:<path>`. bevy-ts SHALL turn the file into a `tsx` description and reconcile it with the same reconciler and state rules as an `app.scene` root:
- each entity becomes an `<entity>` keyed by its name
- each type patch becomes a component element whose fields are controlled
- `Children […]` becomes child entities

`commands.loadLevel(path)` and `commands.unloadLevel(path)` SHALL do the same from a TS system or handler at runtime. Unloading SHALL despawn the level's entities with their descendants.

#### Scenario: Loading the arena
- **WHEN** `game.tsx` calls `app.level("levels/arena.bsn")`, which holds a light and two enemies
- **THEN** the light and both enemies spawn, and `bevy-ts/tsx_tree` lists them under root `level:levels/arena.bsn`

### Requirement: A level places TS scene components with Bevy's syntax
A prefab placeable from a level SHALL be a scene component, a class extending `SceneComponent` whose `static scene(props)` returns the scene's element, as Bevy's `#[derive(SceneComponent)]`. Its name SHALL default to a Rust-style path from its module (`src/enemies.tsx` exporting `Enemy` is `enemies::Enemy`) and MAY be pinned with `static path`. The compile step SHALL read its fields and its `scene` props (types from the props type, defaults from the destructuring), with components' field types.

In a level, `@path::Comp { @prop: value, field: value, … }` SHALL place it, exactly as Bevy's `bsn!` does: `@prop` sets a scene prop, and a plain `field` patches the component's own field. The scene's expansion SHALL be the entity's base, with the scene component added to the root and the entity's later patches layered on top. Props and fields not given SHALL take their defaults. An unknown scene component, prop or field, or a wrongly typed value, SHALL fail the load at its location. A plain function prefab SHALL NOT be placeable from a level.

#### Scenario: Enemy b with more health
- **WHEN** `enemies.tsx` exports `class Enemy extends SceneComponent { static scene({ hp = 100 }: { hp?: f32 }) { ... } }`, and `arena.bsn` has `#EnemyB @enemies::Enemy { @hp: 250.0 } Transform { translation: Vec3 { x: 3.0, z: -4.0 } }`
- **THEN** one enemy spawns at `[3, 0, -4]` with `Health.max` 250 and `Health.hp` 250

#### Scenario: A prop the prefab does not declare
- **WHEN** a level passes `speed: 2.0` to `game::Enemy`
- **THEN** the load fails at that line with "game::Enemy has no prop speed", listing its props

### Requirement: The instance root's Transform is the level's
A component patch on an instance entity SHALL override the expansion's root components field by field. For `Transform`, it SHALL do so whatever the prefab declared, except a bound field (see the bound-field requirement). Placement of a level-instanced prefab SHALL always be a `Transform` override on the instance, never a prop.

#### Scenario: A prefab that sets its own translation
- **WHEN** a prefab's root declares `<Transform translation={[0, 0, -4]} />` and the level patches `translation: Vec3 { x: 3.0 }`
- **THEN** the instance's translation is `[3, 0, -4]`, and `translation`'s provenance is `override` at the level's line

### Requirement: Overrides of a prefab's inner entities merge by name
Inside an instance's `Children […]`, an entry whose name matches a child of the expansion SHALL patch that child: its patches layer after the prefab's, and its own children merge the same way, recursively. An entry whose name matches no child SHALL add a new child. A prefab child with an explicit `key` SHALL be addressed by that key. An unkeyed child SHALL be addressed by its `tsx` segment in a quoted name (`#"entity#1"`), with a dev warning suggesting a key. A level SHALL NOT be able to remove a prefab-internal entity.

#### Scenario: Moving one enemy's eye
- **WHEN** `#EnemyB` has `Children [ #eye Transform { translation: Vec3 { y: 0.8 } } ]`
- **THEN** enemy `b`'s eye is at `y = 0.8` and enemy `a`'s eye keeps the prefab's `0.6`

#### Scenario: Adding a child to an instance
- **WHEN** `#EnemyB`'s `Children […]` has `#hat Transform { … } @hats::Hat { }`
- **THEN** a hat entity spawns under enemy `b` and no other enemy

### Requirement: Nested prefabs and included levels
A TSX prefab that renders another TSX prefab SHALL expand recursively, as in `tsx`. A level SHALL include another level with `:"path.bsn"` as an entity's first entry. The included level's entities become that entity's children, and the including file may override them by name as for prefab children. Overrides SHALL compose through any depth. The outermost file's override wins, then each inner file's, then the TSX prefab's declarations. An inclusion cycle SHALL fail the load, naming the files in the cycle.

#### Scenario: A room included twice
- **WHEN** `arena.bsn` has `#RoomA :"levels/room.bsn"` and `#RoomB :"levels/room.bsn"`, and `#RoomB` overrides `#Lamp`'s intensity
- **THEN** both rooms spawn, only room B's lamp has the new intensity, and editing `room.bsn` updates both rooms

#### Scenario: A cycle
- **WHEN** `a.bsn` includes `b.bsn` and `b.bsn` includes `a.bsn`
- **THEN** the load fails with "inclusion cycle: a.bsn → b.bsn → a.bsn"

### Requirement: Entity identity is the level path and name path
A level entity's node id SHALL be derived from the level path and the `#Name` path from the level's top level. Inside a prefab expansion, `tsx` key segments continue the path. Names SHALL be unique among siblings, and a duplicate SHALL fail the load naming both locations. An unnamed entity SHALL get a positional id among its unnamed siblings and a dev warning. Identity SHALL NOT depend on line numbers, entity order among named siblings, or formatting.

#### Scenario: Reordering entities in the file
- **WHEN** an agent moves `#EnemyA`'s block below `#EnemyB` and saves
- **THEN** no entity respawns and nothing is written to the world

#### Scenario: Duplicate names
- **WHEN** two top-level entities are both `#Crate`
- **THEN** the load fails naming both lines

### Requirement: A level cannot override a bound field
A bound prop has one owner, its binding. A level override of a field the prefab binds SHALL fail the load, with the level's file, line and column, and the binding's site location and expression. This SHALL hold at any depth, and for an instance root's `Transform` too.

#### Scenario: Overriding a bound intensity
- **WHEN** `Lamp` binds `PointLight.intensity` to an alarm signal, and `#Lamp3` in `arena.bsn` overrides `intensity: 20.0`
- **THEN** the load fails at that line with "PointLight.intensity is bound at lamp.tsx:7:20 (() => alarm() ? 8 : 2) and cannot be overridden", and the world is unchanged

### Requirement: Level provenance and protocol reads
Each level-placed entity SHALL carry a `LevelInstance` component with its level path, name path and owner chain. The owner chain lists the level frames, outermost first, then the TSX source chain. tsx provenance SHALL gain `override` with file, line and column. bevy-ts SHALL add these methods to protocol 1.2:
- `bevy-ts/level_list`: open levels with content hash, extensions used and diagnostics.
- `bevy-ts/level_node` (`entity` or level and name path): the owner chain, every override with its location, the instance's props, and `patches`, the components the file adds to the entity that its prefab does not give: a string array of component names, named as `LevelOverride.component` names them (`PointLight`, `enemy::Health`).

For a scene component instance, `level_node` SHALL report every prop its scene declares, beside the component's fields: name, type, the value it takes, and whether the level sets it, with the location of the `@prop` when it does. A prop the level does not set SHALL report its default.

Each `bevy-ts/tsx_tree` node of a level-placed entity, in the tree and in `tsx_tree+watch`, SHALL carry `level`: `file`, the outermost level file; `namePath`, its path from that file's top; and `written`, true when the entity is written in that file rather than coming from a prefab or an included level. A node with no `level` SHALL be TSX-only.

#### Scenario: Inspecting enemy b
- **WHEN** a client calls `bevy-ts/level_node` for enemy `b`'s entity
- **THEN** it gets `levels/arena.bsn`, name path `EnemyB`, prefab `game::Enemy` with `hp: 250`, a `Transform.translation` override at the patch's line, and empty `patches`

#### Scenario: A grunt's props
- **WHEN** `#GruntB` is placed with `@enemies::Grunt { @hp: 250.0 }` and `Grunt`'s scene is `static scene({ hp = 100, speed = 2 })`
- **THEN** `level_node` reports `hp` as f32 `250.0`, overridden at that line, and `speed` as `2`, not overridden

#### Scenario: Querying placed instances
- **WHEN** `arena.bsn` places three `@enemies::Enemy { ... }` and a TS system takes `Query<[Enemy]>`
- **THEN** the system sees three rows, one per placed root, each with its `team` field as the level set it

#### Scenario: Tree rows of the arena
- **WHEN** a client calls `bevy-ts/tsx_tree` on the arena
- **THEN** `#EnemyB`'s node has `level: { file: "levels/arena.bsn", namePath: ["EnemyB"], written: true }`, its eye's node has `namePath: ["EnemyB", "eye"]` and `written: false`, and `#RoomA`'s `#Lamp` from `room.bsn` has `file: "levels/arena.bsn"` and `written: false`

### Requirement: A level opens and closes over the protocol
`bevy-ts/level_open` (`file`: an asset path ending in `.bsn`) SHALL open that file as a level root named `level:<file>`, as `app.level` does, with no TS call. It SHALL read the file and every file it includes from disk, compose and reconcile them before it replies, and answer `{ status: "opened", level }`, where `level` is the file's `bevy-ts/level_list` entry. It SHALL refuse, opening nothing, with `{ status: "failed", reason, diagnostics }`:
- `invalid`: the path does not end in `.bsn`, or leaves the asset root
- `not-found`: no such file
- `load`: the file or a file it includes fails to parse or resolve, with each diagnostic's file, line and column

Opening a level that is already open SHALL answer `opened` with it and SHALL spawn and write nothing. Opening a level SHALL NOT close another; any number of levels MAY be open at once. `level_open` SHALL NOT run any system or `Startup` itself: whether the world's systems run is the mode's rule, so in edit mode the opened level's entities spawn, with their scene components' expansions, and no game system acts on them.

`bevy-ts/level_close` (`file`) SHALL close an open level: its entities despawn with their descendants, and a sidecar's overrides leave its scene. It SHALL answer `{ closed: true }`, or `{ closed: false }` when the level was not open. A level `level_close` closed, whoever opened it, SHALL stay closed for the process until `level_open` names it again: neither a TS generation (for `app.level`) nor a scene's sidecar check SHALL reopen it. `bevy-ts/tsx_tree+watch` SHALL send an opened level's nodes as `added` in the message for its first reconcile, and a closed level's as `removed`.

A level opened this way SHALL be a level like any other: `bevy-ts/tsx_tree`, `level_node`, `level_write`, `level_apply`, picking and `viewport_select` SHALL treat it as they treat an `app.level` level, and it SHALL reload by `level-hot-reload`'s rules, a TS generation's re-expansion included.

Each `bevy-ts/level_list` entry SHALL carry `opener`: `ts` for an `app.level` level, `rust` for `add_level`, `editor` for `level_open`, `sidecar` for a scene's sidecar. A level `level_open` opens is `editor`, whatever else it is (an `app.level` level or a sidecar it reopens after `level_close` included); a level `level_open` names while it is open keeps its opener. A scene's sidecar path that `app.level` declares SHALL mount as a level of its own, never layered over its scene, on its first mount, on a generation that newly declares it and on `level_open`'s reopen.

#### Scenario: Opening the arena in edit mode
- **WHEN** the engine runs in edit mode with no `app.level` call, and a client calls `bevy-ts/level_open` with `levels/arena.bsn`
- **THEN** the reply is `opened` with `opener: "editor"`, `tsx_tree` lists the light and both enemies under root `level:levels/arena.bsn`, and the enemies do not bob

#### Scenario: Opening a level twice
- **WHEN** a client calls `level_open` on `levels/arena.bsn` while it is open
- **THEN** the reply is `opened` with the same level, and no entity spawns, despawns or is written

#### Scenario: A file with a typo
- **WHEN** a client opens a level whose line 12 has `Transfrom { … }`
- **THEN** the reply is `failed` with `reason: "load"` and that location, and `level_list` does not list the level

#### Scenario: A missing file
- **WHEN** a client opens `levels/nope.bsn`, which does not exist
- **THEN** the reply is `failed` with `reason: "not-found"`

#### Scenario: Switching levels
- **WHEN** `levels/arena.bsn` is open, and the studio calls `level_open` on `levels/room.bsn` then `level_close` on `levels/arena.bsn`
- **THEN** the room's entities are in the world, the arena's are despawned, the selection holds none of the arena's entities, and `tsx_tree+watch` sent the room's nodes as `added` and the arena's as `removed`

#### Scenario: Closing a level the game declares
- **WHEN** `game.tsx` calls `app.level("levels/arena.bsn")`, a client closes it, and a TS save applies a new generation
- **THEN** the arena stays closed until a client opens it again, when it answers `opener: "editor"`

#### Scenario: Editing an opened level
- **WHEN** a client opens `levels/room.bsn`, then sends `level_write` with a `set` on `Lamp`'s `PointLight.intensity`
- **THEN** the file changes as for any open level, and the lamp in the world takes the new intensity before the write replies

### Requirement: dev --edit remembers the editor's levels
Under `bevy-ts dev --edit`, the engine SHALL keep `<project>/.bevy-ts/editor.json` as `{ "version": 1, "open": [...], "closed": [...] }`:
- `open`: the asset path of every open level whose opener is `editor`
- `closed`: the asset path of every level `level_close` closed and nothing has reopened

Levels opened by `ts`, `rust` or `sidecar` SHALL NOT go in `open`. The engine SHALL write the file whenever either set changes, to a temporary file renamed over it, so the file is always whole.

Before the first frame, the engine SHALL read the file: it SHALL take `closed` as the levels `level_close` closed, then open each `open` path with opener `editor`. A restored level SHALL compose once its files load and the first TS generation has been tried, applied or failed, as an `add_level` level does. Restored levels SHALL list with opener `editor` in the first `bevy-ts/level_list`, and a restored `closed` level SHALL stay closed as if `level_close` had closed it in this process.

A missing, malformed or unknown-version file SHALL start with both sets empty and a warning, and SHALL NOT fail the boot. A remembered level whose file is gone SHALL be skipped with a warning on stderr and left out of the next write. One that no longer composes SHALL stay open with its error, as an `app.level` level does.

Plain `bevy-ts dev`, `bevy-ts play` and `bevy-ts build` SHALL neither read nor write the file.

#### Scenario: Reopening the room
- **WHEN** a client opens `levels/room.bsn` in a `dev --edit` engine, and the engine is stopped and started again with `--edit`
- **THEN** `.bevy-ts/editor.json` has `open: ["levels/room.bsn"]`, and the new engine's first `level_list` lists the room with `opener: "editor"`

#### Scenario: A closed game level stays closed
- **WHEN** `game.tsx` calls `app.level("levels/arena.bsn")`, a client closes the arena in a `dev --edit` engine, and the engine starts again with `--edit`
- **THEN** the arena does not open, after the TS generation too, until a client opens it, when it answers `opener: "editor"`

#### Scenario: A remembered level was deleted
- **WHEN** `editor.json` has `open: ["levels/gone.bsn", "levels/room.bsn"]` and `levels/gone.bsn` no longer exists
- **THEN** the engine boots with the room open, prints a warning naming `levels/gone.bsn`, and its next write of `editor.json` holds only the room

#### Scenario: A malformed file
- **WHEN** `editor.json` is not valid JSON, or has `version: 2`
- **THEN** the engine boots in edit mode with no level the editor opened, and prints a warning

#### Scenario: Plain dev ignores it
- **WHEN** `editor.json` has `open: ["levels/room.bsn"]` and the engine starts with `bevy-ts dev` without `--edit`
- **THEN** the room is not open, and a `level_open` or `level_close` leaves `editor.json` unchanged
