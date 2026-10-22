## ADDED Requirements

### Requirement: Applying a root at key paths
`bevy-ts-tsx` SHALL apply a root at given key paths only: for each path, the subtree at that path SHALL be reconciled against a given resolved subtree (`reconcile`), or despawned and spawned again from it (`respawn`), or despawned when none is given. Nodes outside the given subtrees SHALL NOT be written, and their change ticks SHALL NOT move. A spawned subtree SHALL be placed under its described parent in description order. A respawned node SHALL clear its tombstone. The root's retained tree, its kept resolved tree and `tsx_tree+watch` SHALL be updated for the given paths as a whole-root reconcile updates them. This is the one primitive for partial applies: a Stop's restore and a level write's reconcile both use it.

#### Scenario: Respawning one enemy
- **WHEN** a root holds enemies `a`, `b` and `c`, and the root is applied at `b` with `respawn`
- **THEN** `b` and its children are new entities with their described values, in `b`'s place among its siblings, and `a` and `c` keep their entities and change ticks

#### Scenario: Reconciling one path
- **WHEN** enemy `b`'s resolved subtree changed one controlled field, and the root is applied at `b` with `reconcile`
- **THEN** only that field is written, under the controlled and initial field rules

#### Scenario: A tombstoned node
- **WHEN** enemy `a` is tombstoned and the root is applied at `a` with `respawn`
- **THEN** `a` exists again and its tombstone is gone

### Requirement: An external root keeps its resolved tree
After applying a root a Rust plugin describes (an external root, such as a level), `bevy-ts-tsx` SHALL keep the resolved tree it applied, shared and not copied, until the root is applied again or unmounted. A partial apply SHALL replace only the subtrees it was given. A restart SHALL drop every kept tree.

#### Scenario: Kept across a frame
- **WHEN** a level root has been applied and no reload has happened since
- **THEN** its kept resolved tree describes every node `tsx_tree` lists for it

### Requirement: The game-start baseline
When the game starts (plugin-api, "Start notification"), `bevy-ts-tsx` SHALL record a baseline for each external root then applied: the world's change tick, the archetype of each of its entities, and the root's apply count. A root applied or unmounted after the baseline SHALL have none. A restart or a Stop SHALL drop every baseline. Given a root with a baseline, tsx SHALL answer the root's changed nodes: those whose entity has a component added or changed after the baseline tick, or an archetype other than the recorded one. Changes tsx made itself through a binding count as changes. The answer SHALL be read before the Stop's rebuild despawns TS-spawned entities, so a child the game spawned and the rebuild despawned marks its parent only if the game's spawn did.

#### Scenario: A system moves one enemy
- **WHEN** the game started with 10,000 enemies applied and a system wrote one enemy's `Transform`
- **THEN** that enemy is the root's one changed node

#### Scenario: A component removed
- **WHEN** a system removed `Health` from enemy `c`
- **THEN** `c` is a changed node

#### Scenario: A root opened during play
- **WHEN** a level is applied after the game started
- **THEN** it has no baseline
