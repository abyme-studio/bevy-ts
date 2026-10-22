## MODIFIED Requirements

### Requirement: Reading components, TS components included
`bevy-ts/get_components` SHALL take the params of stock `world.get_components` (`entity`, `components`, `strict`) and return the same envelope, serving TS components from their native layout and reflected ones through `bevy_ts::reflect_value::read`, in the shape TSX and TS systems use (`Duration` as seconds, `Handle<T>` as its asset path or `null`, 64- and 128-bit integers past ±2^53 as decimal strings). `bevy-ts/get_components+watch` SHALL stream like stock `world.get_components+watch`, including TS components, sending only components whose change tick moved since the last message. This shape SHALL ship under a new protocol minor version.

#### Scenario: Inspecting the cube
- **WHEN** the inspector calls `bevy-ts/get_components` for the cube with `game::Mover` and `Transform`
- **THEN** the result has both, `game::Mover` as `{ "speed": 1.0, "amplitude": 2.0 }`

#### Scenario: A reflected value an inspector can hand back
- **WHEN** the inspector reads a camera's `Projection` and sends the same JSON to `bevy-ts/mutate_components` with an empty path
- **THEN** the call succeeds and the projection is unchanged

### Requirement: Editing a field
`bevy-ts/mutate_components` SHALL take stock `world.mutate_components`'s params (`entity`, `component`, `path`, `value`) for any component. For TS components, `path` SHALL accept `.field`, `.field.x|y|z|w` for vectors and `.field[i]` for arrays; the value SHALL be checked against the field type and hints, written into the column, and the change tick set so watchers and `Changed` see it. For reflected components, `path` SHALL be a reflection path and the value SHALL be written by `bevy_ts::reflect_value::write` over the slot's default, taking every form TSX takes. An invalid path or value SHALL return an error naming the path and the expected type, and write nothing.

#### Scenario: Dragging a number in the inspector
- **WHEN** the inspector sends `{ entity, component: "game::Mover", path: ".amplitude", value: 4 }`
- **THEN** the cube's bob widens on the next frame, and the next `bevy-ts/get_components+watch` message carries `amplitude: 4`

#### Scenario: A wrong type
- **WHEN** the value for `.amplitude` is `"wide"`
- **THEN** the call fails with an error saying `.amplitude` expects `f32`, and the component is unchanged

#### Scenario: A colour from the inspector
- **WHEN** the inspector sends `{ component: "bevy_light::directional_light::DirectionalLight", path: ".color", value: "#ffd7a8" }`
- **THEN** the light's colour is that sRGB colour
