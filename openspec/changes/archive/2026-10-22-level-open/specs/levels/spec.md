## ADDED Requirements

### Requirement: A level opens and closes over the protocol
`bevy-ts/level_open` (`file`: an asset path ending in `.bsn`) SHALL open that file as a level root named `level:<file>`, as `app.level` does, with no TS call. It SHALL read the file and every file it includes from disk, compose and reconcile them before it replies, and answer `{ status: "opened", level }`, where `level` is the file's `bevy-ts/level_list` entry. It SHALL refuse, opening nothing, with `{ status: "failed", reason, diagnostics }`:
- `invalid`: the path does not end in `.bsn`, or leaves the asset root
- `not-found`: no such file
- `load`: the file or a file it includes fails to parse or resolve, with each diagnostic's file, line and column

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
