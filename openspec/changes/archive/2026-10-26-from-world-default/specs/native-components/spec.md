## ADDED Requirements

### Requirement: A FromWorld default is made once
A reflected value whose type has no `ReflectDefault` SHALL start from its `ReflectFromWorld` value on an empty world, made once per type and cloned through reflection after. A type that does not clone through reflection SHALL be made fresh each time, and one whose `from_world` panics SHALL have no default.

#### Scenario: A zoom a frame
- **WHEN** a TS system writes `p.value = { Orthographic: { scale } }` to one camera's `Mut<Projection>` every frame
- **THEN** each write starts from the `OrthographicProjection` made the first time, and no write builds a `World`
