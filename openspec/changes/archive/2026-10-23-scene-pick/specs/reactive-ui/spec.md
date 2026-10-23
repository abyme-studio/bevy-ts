## MODIFIED Requirements

### Requirement: Events from bevy_picking, batched
`onPointerClick`, `onPointerPress`, `onPointerRelease`, `onPointerOver` and `onPointerOut` on any `<entity>` or UI element SHALL register handler ids with Rust, which SHALL observe the matching Bevy 0.20 picking event on that entity (`PointerClick`, `PointerPress`, `PointerRelease`, `PointerOver`, `PointerOut`) and queue it; each prop is an entity-local observer, as `on(...)` is in BSN (operator ruling 2026-10-20). The prop name is `on` plus the Bevy event's name, so the remaining picking events (`PointerDragStart`, `PointerScroll`, ...) follow the same rule when added. Events SHALL bubble up `ChildOf` as Bevy's do: a handler SHALL also run for an event whose original target is a descendant of its entity, including a mesh of a scene spawned under a `WorldAssetRoot`. Queued events SHALL be delivered in the same per-frame host call as subscription changes, in arrival order. A handler SHALL receive the event (`entity`, the entity whose handler runs; `target`, the entity the pointer hit; pointer position; button) and a `ctx` with `commands` and `frame`. A handler SHALL NOT be able to stop propagation: it runs after Bevy's walk.

#### Scenario: Three clicks in one frame
- **WHEN** three buttons are clicked in one frame
- **THEN** one host call runs the three handlers in click order

#### Scenario: A click bubbles to its parent's handler
- **WHEN** an entity with `onPointerClick` has a child entity, and the child is clicked
- **THEN** the parent's handler runs with `entity` the parent and `target` the child

#### Scenario: A click on a glTF scene's mesh
- **WHEN** an entity carries `WorldAssetRoot("models/Fox.glb#Scene0")` and `onPointerClick`, and a mesh of the spawned fox is clicked
- **THEN** the root's handler runs once, with `entity` the root and `target` the mesh's entity
