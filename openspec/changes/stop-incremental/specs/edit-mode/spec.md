## MODIFIED Requirements

### Requirement: Stop rebuilds the edit world from source
A switch from `play` to `edit` (a Stop), paused or not, SHALL throw the play world away and rebuild it from the live generation and the files on disk, as a `restart` reload does, without recompiling: it SHALL re-apply the live generation's module set. The engine SHALL keep no copy of the world. After a Stop:
- every entity TS spawned SHALL be gone;
- every TSX node and tombstone of a scene SHALL be gone, and every scene SHALL mount as if first;
- every level SHALL be as its files describe it, every tombstone of it cleared, as "Stop restores the levels the play changed" says;
- which `Startup` systems ran SHALL be forgotten and `started` SHALL be false;
- `Local` storage, TS resources, TS states and pending TS messages SHALL be reset as a `restart` resets them (hot-reload);
- the JS engine SHALL use a fresh instance;
- `Time<Virtual>`, `Time<Fixed>` and the script frame count SHALL start again from 0, and stay paused;
- the editor camera's pose, the gizmo tools, `ui` and `gamepadUnfocused` SHALL be kept.

A hot reload applied during play SHALL be what Stop rebuilds. The `viewport_mode` answer SHALL come after the rebuild and the levels' restore, and `tsx_tree+watch` and `level+watch` SHALL report what the Stop despawned and spawned, as they report a reconcile. A `restart` reload during play SHALL NOT be a Stop: it SHALL keep `play` and run `Startup` again.

#### Scenario: Stopping the arena
- **WHEN** the arena has played for five seconds, an enemy has been destroyed and the score is 3, and a client sets `edit`
- **THEN** every enemy stands at its level transform, the destroyed one is back, `Score` reads its `insertResource` value, and the answer has `mode: "edit"` and `started: false`

#### Scenario: Stopping a paused game
- **WHEN** the game is paused in play and a client sets `edit`
- **THEN** the world is rebuilt as for any Stop

#### Scenario: A save during play
- **WHEN** during play `enemy.tsx`'s scene gains a child mesh and reloads, then a client sets `edit`
- **THEN** each enemy after the Stop has the new child, and no compile ran for the Stop

#### Scenario: The editor camera stays
- **WHEN** the operator flew the editor camera before Play, then a client sets `play` and `edit`
- **THEN** the editor camera is at the pose it had before Play

#### Scenario: Time starts again
- **WHEN** a client sets `edit` after ten seconds of play, then `play`
- **THEN** a `Startup` system reading `time.elapsed_secs` reads 0

#### Scenario: The tree watch follows the remount
- **WHEN** a client watches `tsx_tree+watch` during play, holding each row's entity by node id, and a client sets `edit`
- **THEN** the next message sends each remounted or respawned row under `added` with its new entity, no level entity is left outside the client's tree (none shows as spawned at runtime), a kept row is not sent, and the watch does not start over with a snapshot

### Requirement: Levels and the selection come back after Stop
A Stop SHALL leave open exactly the levels that were open when play was entered, each with the opener it had then: a level a client opened during play SHALL close, and one a client closed during play SHALL open again. A game that booted in play (no `--edit`) entered no play to go back to: its first Stop SHALL keep the levels open at that moment. Each level SHALL stand as its files describe it before Stop answers: restored ("Stop restores the levels the play changed") or reconciled from disk. The selection SHALL be kept by node id: an item whose node id names an entity after the rebuild SHALL be selected again with that entity, the same one when the Stop kept it, and the change SHALL be streamed with `origin: "engine"`; an item with no node id, or whose node is gone, SHALL be dropped.

#### Scenario: Enemy b stays selected
- **WHEN** enemy `b` of `levels/arena.bsn` is selected, and a client sets `play` then `edit`
- **THEN** after the Stop enemy `b`'s entity is selected (its new one, if the play changed it), and `viewport_selection+watch` sends it when it changed

#### Scenario: A bullet was selected
- **WHEN** during play a client selects a bullet a TS system spawned, then sets `edit`
- **THEN** the selection is empty after the Stop

#### Scenario: The first Stop of a game booted in play
- **WHEN** an engine started without `--edit` has `levels/arena.bsn` open, a client opens `levels/room.bsn`, then sets `edit`
- **THEN** both levels are open after the Stop, each reconciled from disk

#### Scenario: A level opened during play
- **WHEN** `levels/arena.bsn` was open at Play, and during play a client opens `levels/room.bsn`, then sets `edit`
- **THEN** after the Stop only the arena is open, with the opener it had, and `level+watch` sent the room's `closed`

## ADDED Requirements

### Requirement: Stop restores the levels the play changed
A level open when the game started (see scene-reconciler, "The game-start baseline") and still open at the Stop SHALL be kept: its entities the play did not change SHALL stay, with their entity ids and component values, and the Stop SHALL respawn from the level's last resolved tree, without composing or expanding it:
- each subtree whose root entity, or any entity in it, the play changed (a component changed after the baseline, or a component added or removed);
- each node tombstoned while the level was applied, with its subtree.

A level SHALL instead take the full path, remounting as if first, when any of these holds:
- the game started before the level was applied, or there is no baseline (a game booted in play);
- the level was applied again after the baseline (a level reload or a generation applied during play), or it is waiting to be;
- a file it was composed from differs on disk from the text it was composed from;
- it is a scene's sidecar;
- its description holds a JS callback: a binding, a handler, a ref or a `<Show>`/`<For>` slot.

A kept entity SHALL fire no hook or observer and SHALL NOT appear as `Added` to a Rust system. A restored subtree SHALL be stamped (`LevelInstance`, `MissingComponents`) as a reconcile stamps it; the rest SHALL keep their stamps.

#### Scenario: Nothing touched
- **WHEN** `levels/arena.bsn` holds 100 enemies, a client sets `play`, the game reads them and changes none, and a client sets `edit`
- **THEN** every enemy has the entity id it had before Play, and `tsx_tree+watch` reports nothing spawned or despawned

#### Scenario: One enemy moved
- **WHEN** during play a system moves enemy `b` and adds `Stunned` to enemy `c`, then a client sets `edit`
- **THEN** `b` and `c` are respawned with their level values and without `Stunned`, and every other enemy keeps its entity id

#### Scenario: A killed enemy
- **WHEN** during play a system despawns enemy `a`, then a client sets `edit`
- **THEN** `a` exists again with its level values, and `tsx_tree` lists no tombstone

#### Scenario: A stepped game
- **WHEN** an `--edit` engine steps one frame before any Play, which runs `Startup` and moves enemy `b`, then a client sets `play` and `edit`
- **THEN** `b` is back at its level transform

#### Scenario: A level saved during play
- **WHEN** during play `levels/arena.bsn` is saved with enemy `d` added, then a client sets `edit`
- **THEN** the arena remounts as if first and `d` exists

#### Scenario: A level with a handler
- **WHEN** an enemy prefab's scene has an `onClick` handler, and a client sets `play` then `edit`
- **THEN** the arena remounts as if first, and a click on an enemy after the Stop calls the handler of the new isolate

#### Scenario: The selection keeps its entity
- **WHEN** enemy `e` is selected, the play does not change it, and a client sets `edit`
- **THEN** `e` is selected with the same entity, and `viewport_selection+watch` sends no change for it

#### Scenario: A native plugin's Added
- **WHEN** a Rust system counts `Added<Enemy>` and the play changes no enemy, then a client sets `edit` and `play`
- **THEN** the system sees no new `Added<Enemy>` after the Stop
