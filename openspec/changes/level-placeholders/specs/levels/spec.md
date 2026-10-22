## MODIFIED Requirements

### Requirement: A level places TS scene components with Bevy's syntax
A prefab placeable from a level SHALL be a scene component, a class extending `SceneComponent` whose `static scene(props)` returns the scene's element, as Bevy's `#[derive(SceneComponent)]`. Its name SHALL default to a Rust-style path from its module (`src/enemies.tsx` exporting `Enemy` is `enemies::Enemy`) and MAY be pinned with `static path`. The compile step SHALL read its fields and its `scene` props (types from the props type, defaults from the destructuring), with components' field types.

In a level, `@path::Comp { @prop: value, field: value, … }` SHALL place it, exactly as Bevy's `bsn!` does: `@prop` sets a scene prop, and a plain `field` patches the component's own field. The scene's expansion SHALL be the entity's base, with the scene component added to the root and the entity's later patches layered on top. Props and fields not given SHALL take their defaults. An unknown scene component, prop or field, or a wrongly typed value, SHALL become a placeholder at its location, as `level-placeholders` says, and SHALL NOT fail the load under `bevy-ts dev`. A plain function prefab SHALL NOT be placeable from a level.

#### Scenario: Enemy b with more health
- **WHEN** `enemies.tsx` exports `class Enemy extends SceneComponent { static scene({ hp = 100 }: { hp?: f32 }) { ... } }`, and `arena.bsn` has `#EnemyB @enemies::Enemy { @hp: 250.0 } Transform { translation: Vec3 { x: 3.0, z: -4.0 } }`
- **THEN** one enemy spawns at `[3, 0, -4]` with `Health.max` 250 and `Health.hp` 250

#### Scenario: A prop the prefab does not declare
- **WHEN** a level passes `@speed: 2.0` to `game::Enemy`
- **THEN** the enemy expands without it, and its `mismatch` entry lists `"@speed": "2.0"` with "game::Enemy has no prop speed", listing its props

### Requirement: A level opens and closes over the protocol
`bevy-ts/level_open` (`file`: an asset path ending in `.bsn`) SHALL open that file as a level root named `level:<file>`, as `app.level` does, with no TS call. It SHALL read the file and every file it includes from disk, compose and reconcile them before it replies, and answer `{ status: "opened", level }`, where `level` is the file's `bevy-ts/level_list` entry. It SHALL refuse, opening nothing, with `{ status: "failed", reason, diagnostics }`:
- `invalid`: the path does not end in `.bsn`, or leaves the asset root
- `not-found`: no such file
- `load`: the file or a file it includes is broken (a parse error, a duplicate name, an inclusion cycle, a missing include or a bound-field override), with each diagnostic's file, line and column

Missing code SHALL NOT fail an open: the level opens with placeholders (`level-placeholders`), and the reply's `level.diagnostics` carries a warning for each.

Opening a level that is already open SHALL answer `opened` with it and SHALL spawn and write nothing. Opening a level SHALL NOT close another; any number of levels MAY be open at once. `level_open` SHALL NOT run any system or `Startup` itself: whether the world's systems run is the mode's rule, so in edit mode the opened level's entities spawn, with their scene components' expansions, and no game system acts on them.

`bevy-ts/level_close` (`file`) SHALL close an open level: its entities despawn with their descendants, and a sidecar's overrides leave its scene. It SHALL answer `{ closed: true }`, or `{ closed: false }` when the level was not open. A level `level_close` closed, whoever opened it, SHALL stay closed for the process until `level_open` names it again: neither a TS generation (for `app.level`) nor a scene's sidecar check SHALL reopen it. `bevy-ts/tsx_tree+watch` SHALL send an opened level's nodes as `added` in the message for its first reconcile, and a closed level's as `removed`.

A level opened this way SHALL be a level like any other: `bevy-ts/tsx_tree`, `level_node`, `level_write`, `level_apply`, picking and `viewport_select` SHALL treat it as they treat an `app.level` level, and it SHALL reload by `level-hot-reload`'s rules, a TS generation's re-expansion included.

Each `bevy-ts/level_list` entry SHALL carry `opener`: `ts` for an `app.level` level, `rust` for `add_level`, `editor` for `level_open`, `sidecar` for a scene's sidecar. A level `level_open` names while it is open keeps its opener.

#### Scenario: Opening the arena in edit mode
- **WHEN** the engine runs in edit mode with no `app.level` call, and a client calls `bevy-ts/level_open` with `levels/arena.bsn`
- **THEN** the reply is `opened` with `opener: "editor"`, `tsx_tree` lists the light and both enemies under root `level:levels/arena.bsn`, and the enemies do not bob

#### Scenario: Opening a level twice
- **WHEN** a client calls `level_open` on `levels/arena.bsn` while it is open
- **THEN** the reply is `opened` with the same level, and no entity spawns, despawns or is written

#### Scenario: A file with a typo
- **WHEN** a client opens a level whose line 12 has `Transfrom { … }`
- **THEN** the reply is `opened`, the entity spawns without it, and `level.diagnostics` has a warning at line 12 with "unknown component Transfrom (did you mean Transform?)"

#### Scenario: A file that does not parse
- **WHEN** a client opens a level whose line 12 has an unclosed `{`
- **THEN** the reply is `failed` with `reason: "load"` and that location, and `level_list` does not list the level

#### Scenario: A missing file
- **WHEN** a client opens `levels/nope.bsn`, which does not exist
- **THEN** the reply is `failed` with `reason: "not-found"`

#### Scenario: Switching levels
- **WHEN** `levels/arena.bsn` is open, and the studio calls `level_open` on `levels/room.bsn` then `level_close` on `levels/arena.bsn`
- **THEN** the room's entities are in the world, the arena's are despawned, the selection holds none of the arena's entities, and `tsx_tree+watch` sent the room's nodes as `added` and the arena's as `removed`

#### Scenario: Closing a level the game declares
- **WHEN** `game.tsx` calls `app.level("levels/arena.bsn")`, a client closes it, and a TS save applies a new generation
- **THEN** the arena stays closed until a client opens it again, when it answers `opener: "ts"`

#### Scenario: Editing an opened level
- **WHEN** a client opens `levels/room.bsn`, then sends `level_write` with a `set` on `Lamp`'s `PointLight.intensity`
- **THEN** the file changes as for any open level, and the lamp in the world takes the new intensity before the write replies
