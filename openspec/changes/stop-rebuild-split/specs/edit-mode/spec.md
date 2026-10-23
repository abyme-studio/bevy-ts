## MODIFIED Requirements

### Requirement: Stop restores the levels the play changed
A level open when the game started (see scene-reconciler, "The game-start baseline") and still open at the Stop SHALL be kept: its entities the play did not change SHALL stay, with their entity ids and component values, and the Stop SHALL restore from the level's last resolved tree, without composing or expanding it:
- each entity the play changed whose archetype is unchanged and whose changed components tsx can write back (scene-reconciler, "Writing a node back") SHALL be written back in place, keeping its entity id;
- each other subtree whose root entity the play changed (a component changed after the baseline, or a component added or removed) SHALL be respawned;
- each node tombstoned while the level was applied SHALL be respawned with its subtree.

A level SHALL instead take the full path, remounting as if first, when any of these holds:
- the game started before the level was applied, or there is no baseline (a game booted in play);
- the level was applied again after the baseline (a level reload or a generation applied during play), or it is waiting to be;
- a file it was composed from differs on disk from the text it was composed from;
- it is a scene's sidecar;
- its description holds a JS callback: a binding, a handler, a ref or a `<Show>`/`<For>` slot.

A kept entity SHALL fire no hook or observer and SHALL NOT appear as `Added` to a Rust system. A written-back entity SHALL fire Bevy's replace and insert hooks and observers for the components written, and no others. A respawned subtree SHALL be stamped (`LevelInstance`, `MissingComponents`) as a reconcile stamps it; the rest SHALL keep their stamps.

#### Scenario: Nothing touched
- **WHEN** `levels/arena.bsn` holds 100 enemies, a client sets `play`, the game reads them and changes none, and a client sets `edit`
- **THEN** every enemy has the entity id it had before Play, and `tsx_tree+watch` reports nothing spawned or despawned

#### Scenario: One enemy moved
- **WHEN** during play a system moves enemy `b` and adds `Stunned` to enemy `c`, then a client sets `edit`
- **THEN** `b` keeps its entity id and stands at its level transform, `c` is respawned with its level values and without `Stunned`, and every other enemy keeps its entity id

#### Scenario: Every enemy moved
- **WHEN** during play a system moves all 10,000 enemies of a level, then a client sets `edit`
- **THEN** every enemy keeps its entity id and stands at its level transform, and `tsx_tree+watch` reports nothing spawned or despawned

#### Scenario: A Rust-inserted component changed
- **WHEN** a native plugin inserts `Velocity` on every enemy when it is added, and during play a system changes enemy `b`'s `Velocity`, then a client sets `edit`
- **THEN** `b` is respawned, and its `Velocity` is the one the plugin inserts on a new enemy

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
