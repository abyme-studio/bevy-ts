## ADDED Requirements

### Requirement: A reflect-tier enum component is set by its value
A TSX element of a reflect-tier component whose type is an enum SHALL take `value`: a variant's name, or `{ <Variant>: <fields> }`. For a newtype variant, its field SHALL start from the field type's reflected `Default`, else its `FromWorld`, or from its current value when the component already holds that variant, and `<fields>`' keys SHALL set that field's fields, so a prop names only what it changes. A unit variant SHALL take its name. Any other variant SHALL be read whole from `{ <Variant>: <fields> }` through reflection. A changed `value` SHALL write the component in place, as a changed field does. A variant the type lacks, or a newtype field with neither `Default` nor `FromWorld`, SHALL be reported at the element with the component's type path. `bevy-ts types` and `bevy-ts/bevy` SHALL declare reflect-tier struct components with their fields and enum components with this `value`, newtype variants' fields partial, so such elements type-check.

#### Scenario: An orthographic camera
- **WHEN** a TSX entity carries `<Projection value={{ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 13 } } } }} />`
- **THEN** its `Projection` is `Orthographic`, with that scaling mode and the other fields of `OrthographicProjection::from_world` (`near` 0, `far` 1000), and the element type-checks against `bevy-ts/bevy`

#### Scenario: Changing one field of the variant
- **WHEN** a reload changes the height to 20
- **THEN** the same entity's projection is still orthographic, with a height of 20

#### Scenario: A variant the type lacks
- **WHEN** an element writes `<Projection value="Isometric" />`
- **THEN** the element's error names `bevy_camera::projection::Projection` and lists its variants
