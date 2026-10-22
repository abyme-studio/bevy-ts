## ADDED Requirements

### Requirement: Missing code makes a placeholder, not a load error
Under `bevy-ts dev`, bevy-ts's `.bsn` loader SHALL turn each of these into a placeholder entry instead of failing the level:
- `unregistered`: no live code registers the type a patch or a `@Comp { … }` names, or the class is no longer a scene component
- `failed`: the type's code threw (a scene component's `scene()`), or the live generation is the empty one a failed TS build left
- `mismatch`: the code is live but a saved field or prop does not fit it: no such field or prop, a wrongly typed value, or a required prop the file does not set

Every other entry of the file SHALL load as it does today. A level with placeholders SHALL be open and listed like any other level.

#### Scenario: A deleted scene component
- **WHEN** `enemies::Boss` is deleted from TSX, and `arena.bsn` still has `#Boss @enemies::Boss { @hp: 900.0 } Transform { translation: Vec3 { x: 2.0 } }`
- **THEN** the arena loads, the light and the other enemies spawn as before, and `#Boss` spawns at `x = 2` with no `enemies::Boss` component

#### Scenario: A typo in a plain patch
- **WHEN** `#Crate` has `Transform { … }` and `Lootz { table: "chest_rare" }` and no type is named `Lootz`
- **THEN** the crate spawns with its `Transform`, and `Lootz` is an `unregistered` entry

#### Scenario: A scene that throws
- **WHEN** `enemies::Grunt`'s `scene()` throws for `#GruntB`'s props and not for `#GruntA`'s
- **THEN** `#GruntA` expands as usual, and `#GruntB` spawns without its expansion with a `failed` entry whose message is the error

### Requirement: The placeholder entity
A level entity with placeholder entries SHALL spawn under its name path, with every component that did resolve: `Name`, `LevelInstance`, `Transform` and any other live patch. It SHALL carry one reflected component, `bevy_ts_levels::MissingComponents`, holding one entry per placeholder:
- `component`: the name the file writes (`game::Loot`)
- `reason`: `unregistered`, `failed` or `mismatch`
- `message`: one line of why
- `fields`: the saved fields and props, keyed as written, a prop keeping its `@` (`"@hp": "250.0"`), each value its saved text
- `location`: file, line and column of the entry in the level
- `expected`: where its code was expected, when known

The missing type SHALL NOT be on the entity, so no query matches it and no system runs for it. An entity with no placeholder entries SHALL NOT carry `MissingComponents`.

#### Scenario: The boss's marker
- **WHEN** `#Boss` is placed with `@enemies::Boss { @hp: 900.0 }` and `enemies::Boss` is deleted
- **THEN** the boss entity has `MissingComponents` with one entry: `enemies::Boss`, `unregistered`, `fields` `{ "@hp": "900.0" }`, at the `@enemies::Boss` line

#### Scenario: No system runs for it
- **WHEN** a TS system takes `Query<[Loot]>`, `Loot` was renamed to `Treasure`, and the crate still writes `game::Loot { … }`
- **THEN** the system sees no row for the crate

### Requirement: A mismatch applies what fits
For `mismatch` on a component or a scene component whose code is live, bevy-ts SHALL apply the component with the fields and props that fit, the others taking their defaults. Its entry's `fields` SHALL list only the misfit fields and props, and `misfit` SHALL give each one's reason. When a required prop is unset, the scene component SHALL NOT expand: it is a placeholder whose entry names the prop.

#### Scenario: A renamed field
- **WHEN** `Health`'s field `max` is renamed `cap`, and a level has `Health { max: 400.0, hp: 400.0 }`
- **THEN** the entity has `Health` with `hp` 400 and `cap` at its default, and a `mismatch` entry with `fields` `{ "max": "400.0" }` and `misfit` `{ "max": "Health has no field max" }`

#### Scenario: A wrongly typed prop
- **WHEN** `enemies::Enemy`'s `hp` prop becomes `string`, and `#EnemyB` sets `@hp: 250.0`
- **THEN** `#EnemyB` expands with `hp` at its default, and the entry lists `"@hp": "250.0"` as a misfit

### Requirement: A missing scene instance keeps its inner overrides
Under an instance whose scene component is a placeholder, a `Children […]` entry with no base of its own (no include and no `@Comp`) SHALL NOT spawn. It SHALL be kept on the instance's entry as `overrides`: one `{ path, fields }` per entry, `path` the name path below the instance, `fields` its patches' saved text keyed `Component.field`. An entry with a base of its own SHALL spawn as for any instance. When the scene component comes back, the overrides SHALL merge into its expansion by name, as `levels` says.

#### Scenario: The boss's eye
- **WHEN** `#Boss`'s scene component is missing and its `Children […]` has `#eye Transform { translation: Vec3 { y: 0.8 } }` and `#hat @hats::Hat { }`
- **THEN** no `eye` entity spawns, the boss's entry has `overrides` `[{ path: ["eye"], fields: { "Transform.translation": "Vec3 { y: 0.8 }" } }]`, and the hat spawns under the boss

### Requirement: The saved text is kept
bevy-ts SHALL NOT rewrite a placeholder's text. Save, rename, reparent, delete and its undo, duplicate and paste of the entity SHALL keep it byte for byte, because the writer splices text. A `restore` SHALL bring the placeholder back with its entry. Switching between play and edit SHALL keep `MissingComponents` on the entity. A later snapshot-and-restore of the world on Play and Stop SHALL keep it too.

#### Scenario: Renaming the boss
- **WHEN** a client renames `Boss` to `BigBoss` while `enemies::Boss` is missing
- **THEN** the file's `@enemies::Boss { @hp: 900.0 }` text is unchanged, and the entity keeps its marker under its new name

#### Scenario: Deleting and undoing
- **WHEN** a client deletes `#Crate`, whose `game::Loot` is missing, then sends the inverse `restore`
- **THEN** the file equals the original and the crate is back with its `unregistered` entry

#### Scenario: Play and back
- **WHEN** a client sets `play`, then `edit`, with `#Boss` a placeholder
- **THEN** the boss still has `MissingComponents` and `level_node` still reports its entry

### Requirement: The code coming back resolves the placeholder
When a TS generation, a Rust engine or a file change makes the entry resolve, the next reconcile SHALL add the component to the same entity, expand a scene component in place, and drop the entry, with no respawn of the entity. The entity SHALL lose `MissingComponents` when its last entry goes. Nothing beyond the usual `tsx_tree+watch` changes SHALL be sent.

#### Scenario: Undeleting the boss
- **WHEN** `enemies::Boss` is restored in TSX and saved while `#Boss` is a placeholder
- **THEN** the same entity gains `enemies::Boss` and its expansion, `hp` is 900, and its `tsx_tree` node has no `level.missing`

### Requirement: Placeholders over the protocol
Each `bevy-ts/tsx_tree` node of an entity with placeholder entries, in the tree and in `tsx_tree+watch`, SHALL carry `level.missing`: one `{ component, reason }` per entry. `bevy-ts/level_node` SHALL carry `missing`: one `MissingComponent` per entry with `component`, `reason`, `message`, `fields`, `location`, `expected` when known, `misfit` for a `mismatch`, and `overrides` for a scene component with kept overrides. `prefab` SHALL keep naming a missing scene component. `MissingComponent` SHALL have no `schema` in this version. Each entry SHALL also be a warning in the level's diagnostics with its location, on `bevy-ts/level_list`, `bevy-ts/level_open` and `level+watch`'s `loaded`, and in the terminal. These SHALL be additions in the next free protocol minor.

#### Scenario: The boss's row and inspector
- **WHEN** a client calls `tsx_tree` and then `level_node` for `#Boss` while `enemies::Boss` is deleted
- **THEN** the node has `level.missing` `[{ component: "enemies::Boss", reason: "unregistered" }]`, and `level_node` has `prefab` `enemies::Boss` and one `missing` entry with `fields` `{ "@hp": "900.0" }` and the line's location

### Requirement: Where the code was expected
`expected` SHALL be the location of the code bevy-ts expected:
- for `failed` from a throw, the throw's location
- for `failed` from a failed TS build, the file that did not build
- for `unregistered` and `mismatch`, the declaration this process last saw live for that name

It SHALL be absent when the process never saw the type live, as after a restart. bevy-ts SHALL keep no schema of past generations on disk.

#### Scenario: Deleted after the engine started
- **WHEN** `enemies::Boss` was live at `src/enemies.tsx:40:1` and is then deleted
- **THEN** the boss's entry has `expected` `src/enemies.tsx:40:1`

#### Scenario: After a restart
- **WHEN** the engine restarts with `enemies::Boss` still deleted
- **THEN** the boss's entry has no `expected`

### Requirement: Where tolerance stops
These SHALL still fail the level whole, as `level-hot-reload` says: a parse error, a duplicate name, an inclusion cycle, a missing include and an override of a bound field. A shipped game, with bevy-ts in non-dev mode (`dev(false)`), SHALL keep the strict loader: an entry that would be a placeholder SHALL fail its level with its location. `bevy-ts levels check` and `bevy-ts build` SHALL behave as before.

#### Scenario: A cycle is still an error
- **WHEN** `a.bsn` includes `b.bsn`, `b.bsn` includes `a.bsn`, and `a.bsn` also names a deleted class
- **THEN** the load fails with the cycle, and no placeholder is made

#### Scenario: A shipped game with a missing class
- **WHEN** a built game's level names a type its scripts do not register
- **THEN** the level fails to load with that location, as before this change
