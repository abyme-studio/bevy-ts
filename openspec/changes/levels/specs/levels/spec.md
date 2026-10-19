## ADDED Requirements

### Requirement: A level is a reconciled root
`app.level(path)` SHALL register a `.bsn` file under the asset root as a root named `level:<path>`. bevy-ts SHALL turn the file into a `tsx` description and reconcile it with the same reconciler and state rules as an `app.scene` root:
- each entity becomes an `<entity>` keyed by its name
- each type patch becomes a component element whose fields are controlled
- `Children […]` becomes child entities

`commands.loadLevel(path)` and `commands.unloadLevel(path)` SHALL do the same from a TS system or handler at runtime. Unloading SHALL despawn the level's entities with their descendants.

#### Scenario: Loading the arena
- **WHEN** `game.tsx` calls `app.level("levels/arena.bsn")`, which holds a light and two enemies
- **THEN** the light and both enemies spawn, and `bevy-ts/tsx_tree` lists them under root `level:levels/arena.bsn`

### Requirement: A level instances a TSX prefab by name
Every exported TSX prefab, a plain function from one destructured, typed props object to an element, SHALL be callable from a level, with no registration call (operator ruling 2026-10-20). Its name SHALL default to `<module path>#<export>` (`src/enemy.tsx#Enemy`) and MAY be pinned with `Enemy.path = "game::Enemy"`. The compile step (script-runtime) SHALL read its props from the props type, with the same field types as components (`f32`, `Vec3Like`, `Entity`, a string-literal union, ...), and their defaults from the destructuring. In a level, `@name { prop: value, … }` SHALL call that prefab with those props, and its expansion SHALL be the entity's base, with the entity's later patches layered on top. Props not given SHALL take their defaults. An unknown prefab, an unknown prop or a wrongly typed prop SHALL fail the load at its location. A prefab whose props type the compile step cannot read SHALL be usable in TSX and refused by a level with that reason.

#### Scenario: Enemy b with more health
- **WHEN** `enemy.tsx` exports `Enemy = ({ hp = 100 }: { hp?: f32 }) => ...` with `Enemy.path = "game::Enemy"`, and `arena.bsn` has `#EnemyB @game::Enemy { hp: 250.0 } Transform { translation: Vec3 { x: 3.0, z: -4.0 } }`
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
- **WHEN** `#EnemyB`'s `Children […]` has `#hat Transform { … } @game::Hat { }`
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
- `bevy-ts/level_node` (`entity` or level and name path): the owner chain, every override with its location, and the instance's props.

#### Scenario: Inspecting enemy b
- **WHEN** a client calls `bevy-ts/level_node` for enemy `b`'s entity
- **THEN** it gets `levels/arena.bsn`, name path `EnemyB`, prefab `game::Enemy` with `hp: 250`, and a `Transform.translation` override at the patch's line
