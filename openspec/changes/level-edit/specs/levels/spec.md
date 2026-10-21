## MODIFIED Requirements

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

## ADDED Requirements

### Requirement: bevy-ts ships primitive shape scene components
The bevy-ts package SHALL ship three scene components, in every generation whether or not the project imports them:
- `bevy_ts::Cube` with props `size` (Vec3, `[1, 1, 1]`) and `color` (string, `#cccccc`)
- `bevy_ts::Sphere` with props `radius` (f32, `0.5`) and `color`
- `bevy_ts::Plane` with props `size` (Vec2, `[10, 10]`) and `color`

Each SHALL expand to an entity with a `Transform` and the matching `<mesh>`. A level SHALL place them as any scene component, `@bevy_ts::Cube { @size: … }`. `level_catalog` SHALL list them as built-ins, not under `scenes`.

#### Scenario: A cube in a level
- **WHEN** a level has `#Box @bevy_ts::Cube { @color: "#3366cc" }`
- **THEN** a blue unit cube spawns, and `level_node` reports its `size` as the default `[1, 1, 1]`
