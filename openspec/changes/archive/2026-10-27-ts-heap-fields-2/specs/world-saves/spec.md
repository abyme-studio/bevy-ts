## MODIFIED Requirements

### Requirement: TS components migrate on load and unknown ones are kept
On load, each `TsSaved` entry SHALL be written with `TsComponents::encode` when its name is declared in the live generation. Its fields SHALL migrate by the hot-reload rules: a field of the same name and type is kept, a new field takes its default, a field of another type is reset to its default, a field no longer declared is dropped. A `string` or `T[]` field SHALL be kept when its saved value is a value of the live type, whatever a list's length; a list of struct items SHALL be kept only when every item's members match the live item type by name and type. A component that migrated SHALL be reported once per load as a warning in the script log, naming the component and its field lists. An entry whose name is not declared SHALL stay in the entity's `TsSaved`, with one warning per name naming it, and SHALL be saved again by a later save of that entity; it SHALL never be dropped. TS resources with heap fields SHALL save and migrate by the same rules.

#### Scenario: A field was added since the save
- **WHEN** a save holds `game::Health { hp: 3 }` and `Health` now declares `hp` and `max: f32 = 10`
- **THEN** the loaded entity has `hp` 3 and `max` 10, and the log names `game::Health` with `max` added

#### Scenario: A class is gone
- **WHEN** a save holds `game::Shield` and no class is named `game::Shield` any more
- **THEN** the loaded entity keeps `game::Shield`'s JSON in `TsSaved`, the log warns naming `game::Shield`, and saving that entity again writes it back

#### Scenario: Strings and lists round-trip
- **WHEN** an entity with `game::Tag { label: "door", scores: [3, 1, 4] }` is saved and loaded, and `scores` defaults to `[]`
- **THEN** the loaded entity has `label` `"door"` and `scores` `[3, 1, 4]`, and no migration is reported

#### Scenario: A list's item type changed
- **WHEN** a save holds `scores: [1.5]` and `scores` is now `string[]`
- **THEN** the loaded entity has `scores` at its default and the log names `scores` reset

#### Scenario: A struct item gained a member
- **WHEN** a save holds `marks: [{ x: 1, y: 2 }]` and `Point` now also declares `z: f32`
- **THEN** the loaded entity has `marks` at its default and the log names `marks` reset

## ADDED Requirements

### Requirement: Entities in lists are remapped on load
An `Entity` item of a TS list, and an `Entity` member of a struct item, SHALL be remapped through the load's entity map, as a TS `Entity` field is.

#### Scenario: A path of stops
- **WHEN** entity `a` has `game::Path { stops: [b, c] }`, all three are saved, despawned and loaded
- **THEN** the loaded `a`'s `stops` are the loaded `b` and `c`
