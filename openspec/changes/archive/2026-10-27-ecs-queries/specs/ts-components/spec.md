## MODIFIED Requirements

### Requirement: Registration as dynamic ECS components
Each TS component SHALL be registered with `World::register_component_with_descriptor` and `ComponentDescriptor::new_with_layout`: table storage, mutable, summary tick on. A component with no heap field SHALL have no drop function and copy its bytes when cloned. A component with a heap field SHALL have a drop function that frees each heap field and a clone function that deep-copies each one, so a clone owns its own values. Its name in Bevy SHALL be the declared name. It SHALL be queryable by Rust through its `ComponentId` and usable in `QueryBuilder`, `insert_by_id` and `get_by_id`.

#### Scenario: A Rust system reads a TS component
- **WHEN** a Rust system builds a query with `QueryBuilder::ref_id` on `game::Mover`'s `ComponentId`
- **THEN** it iterates every entity carrying `game::Mover` and reads its bytes in the declared layout

#### Scenario: Every heap value is freed once
- **WHEN** entities with a `string` field are spawned, cloned with `cloneAndSpawn`, migrated by a reload, despawned, and the world is dropped
- **THEN** every string built is freed exactly once, and each clone's string is its own

#### Scenario: cloneAndSpawn copies a TS component
- **WHEN** a system calls `commands.entity(e).cloneAndSpawn()` on an entity with `Health.of({ hp: 7 })`
- **THEN** the new entity has `Health` with the same bytes, `hp` 7, and changing one leaves the other as it was
