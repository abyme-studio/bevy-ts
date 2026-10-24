## MODIFIED Requirements

### Requirement: Registration as dynamic ECS components
Each TS component SHALL be registered with `World::register_component_with_descriptor` and `ComponentDescriptor::new_with_layout`: table storage, no drop function, mutable, summary tick on, and `ComponentCloneBehavior::Custom` with a bytewise copy, since a TS component is plain bytes that own nothing. Its name in Bevy SHALL be the declared name. It SHALL be queryable by Rust through its `ComponentId` and usable in `QueryBuilder`, `insert_by_id` and `get_by_id`.

#### Scenario: A Rust system reads a TS component
- **WHEN** a Rust system builds a query with `QueryBuilder::ref_id` on `game::Mover`'s `ComponentId`
- **THEN** it iterates every entity carrying `game::Mover` and reads its bytes in the declared layout

#### Scenario: cloneAndSpawn copies a TS component
- **WHEN** a system calls `commands.entity(e).cloneAndSpawn()` on an entity with `Health.of({ hp: 7 })`
- **THEN** the new entity has `Health` with the same bytes, `hp` 7, and changing one leaves the other as it was
