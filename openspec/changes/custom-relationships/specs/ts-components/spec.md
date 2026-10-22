## MODIFIED Requirements

### Requirement: Registration as dynamic ECS components
Each TS component SHALL be registered with `World::register_component_with_descriptor` and `ComponentDescriptor::new_with_layout`. A plain component, including a relationship source, SHALL have table storage, no drop function, summary tick on and the bytewise clone fn; it SHALL be mutable unless it is a relationship source, which is immutable. A relationship target SHALL be registered as a second kind: table storage, the layout of a Rust `Vec<Entity>`, a drop function for it, mutable, summary tick on, a custom clone fn, no fields and no column JS can see. A source and a target SHALL carry Bevy's `RelationshipAccessor`. Its name in Bevy SHALL be the declared name. It SHALL be queryable by Rust through its `ComponentId` and usable in `QueryBuilder`, `insert_by_id` and `get_by_id`.

#### Scenario: A Rust system reads a TS component
- **WHEN** a Rust system builds a query with `QueryBuilder::ref_id` on `game::Mover`'s `ComponentId`
- **THEN** it iterates every entity carrying `game::Mover` and reads its bytes in the declared layout

#### Scenario: A Rust system reads a TS target
- **WHEN** a Rust system reads `game::LikedBy` by id on an entity and iterates it through its `RelationshipAccessor`
- **THEN** it gets every entity whose `game::Likes` points there
