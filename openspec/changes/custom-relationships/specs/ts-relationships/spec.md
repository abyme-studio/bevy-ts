## ADDED Requirements

### Requirement: Relationship sources and targets are declared as classes
A relationship source SHALL be a class extending `Relationship` whose first field is `target: Entity`; other fields MAY follow. A relationship target SHALL be a class extending `RelationshipTarget(Source)` or `RelationshipTarget(Source, { linkedSpawn: true })`, with no fields. `static allowSelfReferential = true` on a source SHALL allow it to point at its own entity. The load SHALL fail, naming the class, file and line, when: a source's first field is not `target: Entity`; a target declares a field; a target names a class that is not a `Relationship`; two targets name one source; or no target names a source.

#### Scenario: A pair loads
- **WHEN** `game.ts` exports `class Likes extends Relationship { declare target: Entity }` and `class LikedBy extends RelationshipTarget(Likes) {}`
- **THEN** both are components in the world after the module loads, and Bevy's `RelationshipAccessor` on `game::Likes` gives entity offset 0 (8 behind a heap field's header) and on `game::LikedBy` names `game::Likes`

#### Scenario: Target not first
- **WHEN** a source declares `since: f32` before `target: Entity`
- **THEN** loading fails with an error naming the class, `target`, the file and the line

#### Scenario: A source with no target
- **WHEN** `Likes extends Relationship` is declared and no class extends `RelationshipTarget(Likes)`
- **THEN** loading fails naming `Likes`

### Requirement: Relationship sources are immutable
A source SHALL be registered immutable, as Bevy's `ChildOf` is. A query naming `Mut<Source>` SHALL fail the load, with a message saying to insert the source again. Inserting a source on an entity that has one SHALL replace it and move the entity from the old target's list to the new one.

#### Scenario: Writing a source in place
- **WHEN** a system queries `Mut<Likes>`
- **THEN** loading fails with an error naming `Likes` and the re-insert form

#### Scenario: Retargeting
- **WHEN** `a` has `Likes { target: b }` and a system runs `commands.entity(a).insert(Likes.of({ target: c }))`
- **THEN** after the flush `b` has no `LikedBy` and `c`'s `LikedBy` lists `a`

### Requirement: Hooks keep target lists as Bevy's do
bevy-ts SHALL register Bevy's relationship hooks by `ComponentId` on every source and target, with one bevy-ts dispatcher per hook slot, and SHALL run Bevy 0.20's relationship hook logic, honouring `RelationshipHookMode`. Inserting a source SHALL add its entity to the target's list, inserting the target when missing. A source pointing at a missing entity SHALL be removed. A self-reference without `allowSelfReferential` SHALL be removed with a warning. Removing or replacing a source SHALL remove one match from the list, and a list left empty SHALL remove the target. Removing a target SHALL remove the source from every listed entity. Despawning an entity whose target is `linkedSpawn` SHALL despawn every listed entity. These SHALL hold for every insert path: TS commands, TSX, Rust and the protocol.

#### Scenario: Insert and remove
- **WHEN** `a` and `b` get `Likes { target: t }` and then `a`'s `Likes` is removed
- **THEN** `t`'s `LikedBy` lists `a` and `b`, then only `b`; removing `b`'s too removes `LikedBy` from `t`

#### Scenario: Removing the target
- **WHEN** `t`'s `LikedBy` lists `a` and `LikedBy` is removed from `t`
- **THEN** `a` no longer has `Likes`

#### Scenario: Linked despawn
- **WHEN** `Owns` has `RelationshipTarget(OwnedBy, { linkedSpawn: true })`, `t` owns `a` and `b`, and `t` is despawned
- **THEN** `a` and `b` are despawned

#### Scenario: Unlinked despawn
- **WHEN** `t`'s `LikedBy` (not linked) lists `a` and `t` is despawned
- **THEN** `a` survives without `Likes`

#### Scenario: A Rust insert
- **WHEN** a Rust system inserts `game::Likes` by id with `target` set to `t`
- **THEN** `t`'s `LikedBy` lists that entity

### Requirement: Targets are read-only query data
A TS query MAY name a target as data (`Query<[LikedBy]>`, `Option<LikedBy>`) and as a filter (`With`, `Without`, `Changed`, `Added`). Each row SHALL get a read-only `TargetList` of `Entity` with `length`, `at(i)`, `includes`, iteration and `toArray()`, reused for the next row as every row accessor is. `Mut<Target>` SHALL fail the load. Rust SHALL pack each matched table's lists into one offsets array and one entity array before the JS call, so reading a target costs no host call per row.

#### Scenario: Reading a target
- **WHEN** `t`'s `LikedBy` lists `a` and `b` and a system iterates `Query<[Entity, LikedBy]>`
- **THEN** the row for `t` has a list of length 2 holding `a` and `b`

#### Scenario: Writing a target
- **WHEN** a system queries `Mut<LikedBy>`
- **THEN** loading fails with an error naming `LikedBy`

### Requirement: Relationships clone as Bevy's do
Cloning an entity with a source SHALL give the clone the source, its `target` mapped through the clone's entity mapper, and add the clone to the target's list. Cloning an entity with a target SHALL give the clone an empty list; with linked cloning and `linkedSpawn`, each listed entity SHALL be cloned and point at the clone. Moving a target SHALL retarget its sources to the new entity.

#### Scenario: Clone a source
- **WHEN** `a` has `Likes { target: t }` and a system runs `cloneAndSpawn(a)`
- **THEN** the clone has `Likes { target: t }` and `t`'s `LikedBy` lists `a` and the clone

#### Scenario: Clone a linked target
- **WHEN** `t` owns `a` through a `linkedSpawn` pair and `t` is cloned with linked cloning
- **THEN** a clone of `a` exists, owned by the clone of `t`, and `a` is still owned by `t`

### Requirement: Targets are derived, not saved
Levels and scene saves SHALL NOT write a target. A target SHALL be rebuilt from its sources by the hooks.

#### Scenario: Saving a level
- **WHEN** a level entity carries a target component
- **THEN** the saved file has no entry for it
