## MODIFIED Requirements

### Requirement: Simulation-despawned nodes stay despawned
When an entity tsx spawned is despawned by anything other than tsx, the reconciler SHALL tombstone its node, and later reloads SHALL NOT respawn it. `bevy-ts/reload` with `restart: true` SHALL clear every tombstone and the retained tree, so every scene mounts as if first. A Stop (edit-mode) SHALL clear every tombstone too: a kept root's tombstoned nodes SHALL be respawned from its resolved tree, keeping its other nodes, and every other root SHALL mount as if first. A node whose key path changes is a new node and SHALL spawn.

#### Scenario: A killed enemy across a reload
- **WHEN** a system despawns enemy `a` and an unrelated edit is saved
- **THEN** `a` is not respawned, and `bevy-ts/tsx_tree` lists it as tombstoned

#### Scenario: A restart
- **WHEN** `bevy-ts/reload` is called with `restart: true`
- **THEN** enemy `a` exists again with `hp` from its `initialHp`

#### Scenario: A Stop keeps the others
- **WHEN** a level's enemy `a` was despawned during play and enemy `b` was not changed, and a client sets `edit`
- **THEN** `a` exists again with `hp` from its `initialHp`, and `b` keeps its entity
