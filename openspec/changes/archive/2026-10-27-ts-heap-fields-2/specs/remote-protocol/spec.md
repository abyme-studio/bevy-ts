## MODIFIED Requirements

### Requirement: Editing a field
`bevy-ts/mutate_components` SHALL take stock `world.mutate_components`'s params (`entity`, `component`, `path`, `value`) for any component. For TS components, `path` SHALL accept `.field`, `.field.x|y|z|w` for vectors and `.field[i]` for fixed arrays; the value SHALL be checked against the field type and hints, written into the column, and the change tick set so watchers and `Changed` see it. A `string` or `T[]` field SHALL be written whole through `.field`, its value a string or an array of its item type. From protocol 1.36.0, `.field[i]` on a `T[]` field SHALL write item `i` in place, its value one item of the list's type, and `.field[i].member` SHALL write one member of a struct or vector item; an index past the end SHALL fail naming the list's length. For reflected components, `path` SHALL be a reflection path and the value SHALL be written by `bevy_ts::reflect_value::write` over the slot's default, taking every form TSX takes. An invalid path or value SHALL return an error naming the path and the expected type, and write nothing.

#### Scenario: Dragging a number in the inspector
- **WHEN** the inspector sends `{ entity, component: "game::Mover", path: ".amplitude", value: 4 }`
- **THEN** the cube's bob widens on the next frame, and the next `bevy-ts/get_components+watch` message carries `amplitude: 4`

#### Scenario: A wrong type
- **WHEN** the value for `.amplitude` is `"wide"`
- **THEN** the call fails with an error saying `.amplitude` expects `f32`, and the component is unchanged

#### Scenario: A colour from the inspector
- **WHEN** the inspector sends `{ component: "bevy_light::directional_light::DirectionalLight", path: ".color", value: "#ffd7a8" }`
- **THEN** the light's colour is that sRGB colour

#### Scenario: Renaming in the inspector
- **WHEN** the inspector sends `{ component: "game::Tag", path: ".label", value: "gate" }`
- **THEN** the next `bevy-ts/get_components` answer has `label: "gate"` and a TS system reads `"gate"`

#### Scenario: An item of a list
- **WHEN** the inspector sends `{ component: "game::Tag", path: ".scores[0]", value: 1 }` on `scores: [5, 6]`
- **THEN** the next `bevy-ts/get_components` answer has `scores: [1, 6]` and `Changed<Tag>` matches the entity

#### Scenario: An item past the end
- **WHEN** the inspector sends `.scores[2]` on a list of 2
- **THEN** the call fails saying `.scores` has 2 items, and the component is unchanged

### Requirement: Schemas carry string and list field types
The layout in a TS component's `x-bevy-ts` SHALL name a `string` field's type `{ kind: "string" }` and a `T[]` field's type `{ kind: "list", of: <item type> }`, where the item type is any field type a list holds: a number kind, `bool`, `string`, a vector, `entity`, an enum, or `{ kind: "struct", fields: [...] }` with each member's name and type. Its `properties` SHALL be a JSON Schema string or array of the item type. The protocol schema file SHALL list these kinds under a new protocol minor version, 1.34.0 (`struct` with its `fields`, and vectors, `entity` and enums as list items), noted in the changelog.

#### Scenario: An editor types a list
- **WHEN** the inspector asks `bevy-ts/schema` for `game::Tag`
- **THEN** `scores` has `{ kind: "list", of: { kind: "u32" } }` and an array property of integers

#### Scenario: An editor types a list of structs
- **WHEN** the inspector asks `bevy-ts/schema` for `game::Path` with `marks: Point[]`
- **THEN** `marks` has `{ kind: "list", of: { kind: "struct", fields: [x: f32, y: f32] } }` and an array property of objects with number properties `x` and `y`
