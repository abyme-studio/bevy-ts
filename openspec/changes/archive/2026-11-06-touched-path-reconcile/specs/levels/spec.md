## ADDED Requirements

### Requirement: Composing a level at key paths
levels SHALL compose a level filtered to key paths: it SHALL read the level and its includes down to each path and build only the subtrees at those paths, with their ancestors' overrides, owner frames and include merges as a whole compose gives them. Only scene component instances inside those subtrees SHALL be expanded. A path that names nothing SHALL compose to nothing. The subtrees SHALL equal the same subtrees of a whole compose. A whole compose SHALL be the same walk with no filter.

#### Scenario: One instance of many
- **WHEN** a level with 10,000 instances is composed at `G7`
- **THEN** one description comes back, `G7`'s, equal to `G7` in the whole compose, and one instance is expanded

#### Scenario: A path under an include
- **WHEN** `arena.bsn` includes `room.bsn` as `Room`, and is composed at `Room/Lamp`
- **THEN** `Lamp` comes back with `room.bsn`'s patches and `arena.bsn`'s overrides of it

#### Scenario: A removed path
- **WHEN** a level is composed at `G7` and the file no longer names `G7`
- **THEN** `G7` composes to nothing

### Requirement: A level's state is kept by key path
A level's owner chains, scene placements, placeholder entries, warnings and asset paths SHALL be kept by the key path they belong to, so a partial reconcile replaces those at and under its paths and keeps the rest. Warnings that belong to no entity SHALL be replaced only by a whole reconcile. `loaded` SHALL list the level's diagnostics in key path order.

#### Scenario: A placeholder fixed in one instance
- **WHEN** `G3` and `G7` each have a placeholder entry, and a write removes `G7`'s
- **THEN** `G7` has no `MissingComponents`, and `G3` keeps its entry and its warning
