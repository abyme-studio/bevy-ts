## MODIFIED Requirements

### Requirement: A written reflected value is the JSON over the default
When TSX writes a reflected value, keys the JSON leaves out SHALL take their default: the field's value in the component's `ReflectDefault`, else in a value its `ReflectFromWorld` makes, or for a newly chosen variant, the variant's default (a newtype variant's inner `ReflectDefault`). A key left out SHALL NOT keep the previous value. A struct or tuple variant with no default SHALL be given whole. An unknown key SHALL fail with an error naming it. A component with neither SHALL merge over its current value instead, and TSX SHALL NOT insert it.

#### Scenario: A field dropped from a prop
- **WHEN** `<Projection value={{ Orthographic: { scale: 2 } }} />` is edited to `<Projection value={{ Orthographic: {} }} />` and reloaded
- **THEN** `scale` is back at `OrthographicProjection`'s default

#### Scenario: A typo
- **WHEN** a prop is `{ Orthographic: { scael: 2 } }`
- **THEN** the scene fails to load with an error naming `scael` and `OrthographicProjection`

#### Scenario: A component with only FromWorld
- **WHEN** a scene has `<AudioPlayer value="tone.ogg" />`, whose type has no `Default` and a `ReflectFromWorld` the engine registers
- **THEN** the entity gets an `AudioPlayer` whose handle is `tone.ogg`
