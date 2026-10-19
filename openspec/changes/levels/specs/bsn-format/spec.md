## ADDED Requirements

### Requirement: bevy-ts reads upstream's `.bsn` file grammar
bevy-ts SHALL accept every file that the vendored `bevy_bsn` parser (Bevy PR #26043) accepts, and SHALL parse it to an equivalent tree. That grammar covers:
- `#Name` entries
- one leading cached inclusion `:"path.bsn"`
- type patches with named, tuple or unit fields, including enum variants and `a::b::C` paths
- `Children [ … ]` and other relationship lists, with `--` between entities
- top-level entities separated by `--`
- float, int, bool, string and nested-type values

A differential test SHALL parse every fixture and every example level with both parsers and compare the trees.

#### Scenario: An upstream-only file
- **WHEN** `room.bsn` uses only `#Name`, type patches, `Children […]` and `--`
- **THEN** both parsers produce the same tree, and `bevy-ts levels check --upstream room.bsn` reports no extensions

### Requirement: Four named extensions, and nothing else
Beyond upstream's grammar, bevy-ts SHALL accept exactly four extensions:
- E1 `//` and `/* */` comments
- E2 prefab instances `@path::Prefab { prop: value, … }`
- E3 a `#Name` child of an inclusion merges with the included child of that name
- E4 quoted names `#"…"`

It SHALL reject anything else upstream's file grammar lacks, with file, line, column and the construct named. That includes `{expr}`, closures, `on(…)`, `~` templates, ranges, struct update and list or map values. `bevy-ts levels check --upstream <file>` SHALL list the extensions a file uses, with their locations.

#### Scenario: An expression in a file
- **WHEN** a level contains `Transform { translation: {Vec3::ZERO} }`
- **THEN** the load fails at that line and column with "expressions are not supported in .bsn files", and the level stays as it was

#### Scenario: Listing extensions
- **WHEN** `levels check --upstream arena.bsn` runs on a file with comments and two `@game::Enemy` instances
- **THEN** it reports E1 and E2 with each location

### Requirement: Lossless parse
bevy-ts's parse SHALL keep every token, comment and whitespace run with its byte span. Printing an unedited tree SHALL give back the input byte for byte, for every file the parser accepts.

#### Scenario: Round trip of a hand-formatted file
- **WHEN** a file with comments, blank lines, mixed one-line and multi-line patches and trailing commas is parsed and printed
- **THEN** the output equals the input exactly

### Requirement: Patch semantics follow upstream
A type patch SHALL write only the fields it names. Entries on one entity SHALL apply in order, and a later patch SHALL override an earlier one field by field. An inclusion (`:"…"` or `@Prefab`) SHALL be the entity's first entry after its name, and the patches that follow it SHALL override what it produced. A component named without fields SHALL insert the component with its defaults for fields no earlier entry set.

#### Scenario: Layered patches
- **WHEN** an entity has `@game::Enemy { }` and then `Transform { translation: Vec3 { x: 3.0 } }`, and the prefab sets translation `[0, 0, -4]`
- **THEN** the entity's translation is `[3, 0, -4]`

#### Scenario: An inclusion after a patch
- **WHEN** an entity has a `Transform` patch and then `:"room.bsn"`
- **THEN** the load fails with "an inclusion must be the first entry", at the inclusion's location

### Requirement: Values are typed by the registry and by TS layouts
Field values SHALL be checked against the component's type: Rust components through Bevy's type registry, and TS components through their `component()` layouts. A type path SHALL resolve by its full path, or by its short name when the registry has exactly one match. A wrong type, an unknown field or an ambiguous short name SHALL fail the load with the location and, for ambiguity, every candidate.

#### Scenario: Ambiguous short name
- **WHEN** two registered crates both define `Marker` and a level writes `Marker`
- **THEN** the load fails naming both full paths

### Requirement: Asset references are strings in handle fields
A string value in a `Handle<T>` field SHALL be an asset path relative to the asset root, label included (`"models/crate.glb#Scene0"`). A path starting `./` or `../` SHALL resolve against the level file's own directory. A path that does not exist SHALL be a warning with file, line and column, and SHALL NOT fail the load. `bevy-ts levels check` SHALL list every unresolved asset path in a project.

#### Scenario: A relative mesh path
- **WHEN** `levels/rooms/a.bsn` sets a mesh handle to `"./crate.glb#Mesh0"`
- **THEN** it loads `levels/rooms/crate.glb#Mesh0` from the asset root

#### Scenario: A missing texture
- **WHEN** a level names `"textures/gone.png"`
- **THEN** the level loads, the entity's handle is unresolved, and `level+watch` and the terminal report the path with its location

### Requirement: Adopting Bevy's loader is a deletion for extension-free files
When Bevy ships a `.bsn` asset loader, bevy-ts SHALL be able to replace its parser and loader with Bevy's behind the same level root, with no change to any level file that uses no extension. `bevy-ts levels migrate` SHALL rewrite files that use extensions into the upstream equivalent forms once those exist.

#### Scenario: A file with no extensions
- **WHEN** bevy-ts switches to Bevy's loader
- **THEN** `room.bsn`, which uses no extension, loads into the same entities without being edited
