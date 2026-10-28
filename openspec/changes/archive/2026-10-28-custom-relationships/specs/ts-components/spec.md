## MODIFIED Requirements

### Requirement: Registration as dynamic ECS components
Each TS component SHALL be registered with `World::register_component_with_descriptor` and `ComponentDescriptor::new_with_layout`: table storage, summary tick on, mutable unless it is a relationship source, which is immutable as `ChildOf` is. A component with no heap field SHALL have no drop function and copy its bytes when cloned. A component with a heap field SHALL have a drop function that frees each heap field and a clone function that deep-copies each one, so a clone owns its own values. A relationship source's clone function SHALL also map its `target` through the clone's entity mapper. A relationship target SHALL be registered as a second kind: the layout of a Rust `Vec<Entity>`, a drop function for it, mutable, a custom clone function, no fields and no column JS can see. A source and a target SHALL carry Bevy's `RelationshipAccessor`. Its name in Bevy SHALL be the declared name. It SHALL be queryable by Rust through its `ComponentId` and usable in `QueryBuilder`, `insert_by_id` and `get_by_id`.

#### Scenario: A Rust system reads a TS component
- **WHEN** a Rust system builds a query with `QueryBuilder::ref_id` on `game::Mover`'s `ComponentId`
- **THEN** it iterates every entity carrying `game::Mover` and reads its bytes in the declared layout

#### Scenario: Every heap value is freed once
- **WHEN** entities with a `string` field are spawned, cloned with `cloneAndSpawn`, migrated by a reload, despawned, and the world is dropped
- **THEN** every string built is freed exactly once, and each clone's string is its own

#### Scenario: cloneAndSpawn copies a TS component
- **WHEN** a system calls `commands.entity(e).cloneAndSpawn()` on an entity with `Health.of({ hp: 7 })`
- **THEN** the new entity has `Health` with the same bytes, `hp` 7, and changing one leaves the other as it was

#### Scenario: A Rust system reads a TS target
- **WHEN** a Rust system reads `game::LikedBy` by id on an entity and iterates it through its `RelationshipAccessor`
- **THEN** it gets every entity whose `game::Likes` points there
