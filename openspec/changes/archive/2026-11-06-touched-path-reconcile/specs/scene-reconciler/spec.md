## MODIFIED Requirements

### Requirement: Applying a root at key paths
`bevy-ts-tsx` SHALL apply a root at given key paths only: for each path, the subtree at that path SHALL be reconciled against a given resolved subtree (`reconcile`), or despawned and spawned again from it (`respawn`), or despawned when none is given. Nodes outside the given subtrees SHALL NOT be written, and their change ticks SHALL NOT move. A spawned subtree SHALL be placed under its described parent in description order, in the world and in the root's kept resolved tree. A respawned node SHALL clear its tombstone. The root's retained tree, its kept resolved tree and `tsx_tree+watch` SHALL be updated for the given paths as a whole-root reconcile updates them. This is the one primitive for partial applies: a Stop's restore and a level write's reconcile both use it.

#### Scenario: Respawning one enemy
- **WHEN** a root holds enemies `a`, `b` and `c`, and the root is applied at `b` with `respawn`
- **THEN** `b` and its children are new entities with their described values, in `b`'s place among its siblings, and `a` and `c` keep their entities and change ticks

#### Scenario: Reconciling one path
- **WHEN** enemy `b`'s resolved subtree changed one controlled field, and the root is applied at `b` with `reconcile`
- **THEN** only that field is written, under the controlled and initial field rules

#### Scenario: A tombstoned node
- **WHEN** enemy `a` is tombstoned and the root is applied at `a` with `respawn`
- **THEN** `a` exists again and its tombstone is gone

#### Scenario: A new node between two
- **WHEN** a root holds `a` and `c`, and is applied at a new `b` described between them, with `reconcile`
- **THEN** `tsx_tree` and the kept resolved tree list `a`, `b`, `c` in that order

## ADDED Requirements

### Requirement: A rekey moves the kept tree
When a node of an external root moves to a new key path (`rekey`), its node in the root's kept resolved tree SHALL move with it, so a later partial apply at the new path finds the node's own description.

#### Scenario: A rename then an edit
- **WHEN** `G7` is rekeyed to `Boss`, then the root is applied at `Boss` with a description equal to `G7`'s
- **THEN** nothing is written, and `Boss` keeps `G7`'s entity
