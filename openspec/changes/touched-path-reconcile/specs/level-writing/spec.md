## ADDED Requirements

### Requirement: A write reconciles only what it touched
After a write, levels SHALL reconcile, in each open level the file is in, only the subtrees at the name paths the batch touched: an edited entity, an added or restored one, a deleted one, a reparented one's old and new place, and a renamed one under its new name with the positional names the write shifted. A touched path inside a scene component instance SHALL reconcile the whole instance. Every other entity of the level SHALL keep its entity, its components and its change ticks, and SHALL NOT be composed, expanded or resolved again. The world after the write SHALL be the one a reconcile of the whole level makes.

#### Scenario: Moving one of ten thousand enemies
- **WHEN** a level holds 10,000 `Grunt` instances and the viewport writes `G7`'s `Transform.translation.x`
- **THEN** one instance is expanded, only `G7`'s nodes are checked, `G7`'s entity has the new translation, and no other enemy's change ticks move

#### Scenario: Adding an enemy among others
- **WHEN** `add_instance` of `enemies::Grunt` is written at index 3 of the top level
- **THEN** the new enemy spawns at index 3 in `tsx_tree`, and the other enemies keep their entities

#### Scenario: Deleting an enemy
- **WHEN** `delete` of `G7` is written
- **THEN** `G7` and its children despawn, and the other enemies keep their entities

#### Scenario: Overriding a prefab's inner entity
- **WHEN** a write sets `G7/eye`'s `Transform` in the level
- **THEN** `G7`'s instance is composed and expanded again, and only `G7`'s nodes are checked

#### Scenario: The same as the full path
- **WHEN** any write op is applied once with partial reconciles and once with them off, from the same files
- **THEN** `tsx_tree`, each node's components and each `LevelInstance` are the same

### Requirement: When a write reconciles the whole level
A write SHALL reconcile every open level it touches as a whole, as before this change, when: the batch adds or removes an include, or changes the file's single top; the level is a sidecar, waiting to reconcile, failed its last reconcile, or has no kept resolved tree; a touched subtree, as it was or as it is now, holds a binding, handler, ref or slot; a touched path does not map into the level; or the partial compose or resolve fails. A failure SHALL then be reported as a whole-level reconcile reports it. A write the disk rebased SHALL reconcile the disk's change whole first, then its own paths.

#### Scenario: A clickable enemy
- **WHEN** `Grunt`'s scene sets an `onClick` handler and a write sets `G7`'s `Transform`
- **THEN** the level reconciles whole, and the edit applies

#### Scenario: Adding an include
- **WHEN** a write adds `@include("levels/room.bsn")` as a new entity
- **THEN** the level reconciles whole, and the room's entities spawn

### Requirement: Edit step timings
levels SHALL record, for the last write, the time of each step (writer, parse, compose, expand, reconcile, stamp, total), whether it reconciled whole and why, how many instances it expanded and how many nodes it checked. The record SHALL NOT be part of the protocol.

#### Scenario: A partial edit's record
- **WHEN** a write sets one field of `G7` in a level of 1,000 instances
- **THEN** the record says partial, one instance expanded, and `G7`'s two nodes checked
