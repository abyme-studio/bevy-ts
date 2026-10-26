# world-saves Specification

## Purpose
TBD - created by archiving change world-saves. Update Purpose after archive.

## Requirements

### Requirement: Save and load are two commands
`Commands` SHALL have `saveWorld(name, options?)` and `loadWorld(name, options?)`. Each SHALL be a command op applied with `&mut World` when the buffer is applied, as `spawn` is. `name` SHALL match `[A-Za-z0-9_-]{1,64}`; any other name SHALL fail the call in TS with an error naming the rule, and nothing is written. Neither call SHALL return a value: the outcome arrives as a message (below).

#### Scenario: A save from a system
- **WHEN** an `Update` system calls `commands.saveWorld("slot1")`
- **THEN** after the buffer is applied a save of the world is under way, and the system ran without waiting for it

#### Scenario: A name with a path in it
- **WHEN** a script calls `commands.saveWorld("../x")`
- **THEN** the call throws an error naming the allowed characters, and no file is written

### Requirement: The saved set is the Save-marked entities by default
bevy-ts SHALL provide a marker component `Save` (a reflected Rust component, `#[reflect(Component)]`, exported from `bevy-ts`). With no `entities` option, a save SHALL hold exactly the entities that have `Save`, disabled ones included, and no others. An `entities` option SHALL replace that set: a query (every entity it matches) or an array of entities. `Save` itself SHALL be saved, so a loaded entity has it again. Resource entities SHALL never be saved as entities.

#### Scenario: Unmarked entities stay out
- **WHEN** the world has 3 entities with `Save` and 5 without, and a script saves with no options
- **THEN** the file holds 3 entities

#### Scenario: An explicit set
- **WHEN** a script saves with `entities: [a, b]`, where neither has `Save`
- **THEN** the file holds `a` and `b` and nothing else

### Requirement: One builder saves every component, Rust and TS
A save SHALL be built by Bevy's `DynamicWorldBuilder` over the saved set, with the `AppTypeRegistry`. Every component with `ReflectComponent` in the registry SHALL be saved through it, by Bevy's serializer; bevy-ts SHALL NOT write a serialiser for any Rust type. TS components, which have no `TypeId`, SHALL be added to each saved entity of that `DynamicWorld` as one reflected component `TsSaved`, a map from the TS component's name to its JSON as `value::decode_component` writes it. An entity with no TS component SHALL have no `TsSaved`. A Rust component without `ReflectComponent` (a native `#[export]` without `#[reflect(Component)]`) SHALL NOT be saved, and docs/saves.md SHALL say so.

#### Scenario: A Rust and a TS component on one entity
- **WHEN** an entity has `Save`, `Transform` and the TS component `game::Health { hp: 3 }`, and is saved
- **THEN** the file holds its `Transform` as Bevy writes it and a `TsSaved` with `game::Health` mapped to `{"hp":3}`

### Requirement: Filters by class
`allow: [..]` and `deny: [..]` SHALL take component classes, Rust (by their reflect `path`) or TS (by their name). With `allow`, only the listed components SHALL be saved; `deny` SHALL leave the listed ones out; with both, `deny` SHALL apply after `allow`. Rust classes SHALL become a `WorldFilter` by `TypeId` from the registry; TS classes SHALL filter the entries of `TsSaved`. `resources: [..]` SHALL name the resources to save, Rust or TS; with no `resources`, a save SHALL hold no resources. A class that is neither a registered Rust component nor a declared TS component SHALL fail the call naming it.

#### Scenario: Deny a Rust and a TS component
- **WHEN** entities with `Save`, `Transform`, `GlobalTransform` and `game::Health` are saved with `deny: [GlobalTransform, Health]`
- **THEN** the file holds their `Transform` and neither `GlobalTransform` nor `game::Health`

#### Scenario: Allow one TS component
- **WHEN** the same entities are saved with `allow: [Health]`
- **THEN** the file holds `game::Health` for each and no Rust component but `Save`

#### Scenario: A Rust and a TS resource
- **WHEN** a script saves with `resources: [ClearColor, Score]`, `Score` a TS resource
- **THEN** the file holds both, and no other resource

### Requirement: The file is Bevy's own world RON
A save SHALL be one file `<name>.scn.ron`, written by `DynamicWorld::serialize` with the app's type registry, so Bevy's `WorldAssetLoader` reads it. TS resources SHALL ride as one reflected resource `TsSavedResources`, as `TsSaved` does for entities. Handles SHALL be written as their asset path, as Bevy writes them. The world walk and serialisation SHALL run in the op; the write SHALL run on Bevy's `IoTaskPool`, to a temporary file of its own in the same folder renamed over the old one, so a crash mid-write, or two saves of one name at once, never leaves a broken save.

#### Scenario: Bevy reads a bevy-ts save
- **WHEN** a save holding only Rust components is loaded by Bevy's `WorldAssetLoader` in a plain Bevy app with the same types registered
- **THEN** it loads without error and spawns the same entities

#### Scenario: A failed write keeps the old save
- **WHEN** a save of `slot1` exists and a second save of `slot1` fails before its rename
- **THEN** `slot1.scn.ron` is the first save, whole

#### Scenario: Two saves of one name at once
- **WHEN** two saves of `slot1` with different entities are started in the same frame
- **THEN** both report no error and `slot1.scn.ron` loads as one of them, whole

### Requirement: Load adds fresh entities and remaps references
`loadWorld(name)` SHALL read the file on the `IoTaskPool`, deserialise it with Bevy's `WorldDeserializer` (its `load_from_path` the `AssetServer`), and write it into the world with `DynamicWorld::write_to_world` and a fresh `EntityHashMap`, in a later frame. Each saved entity SHALL become a new entity; the live world SHALL be left as it is (load adds; it does not despawn). Rust `Entity` fields SHALL be remapped by Bevy. TS `Entity` fields SHALL be remapped through the same map; a TS `Entity` field naming an entity outside the save SHALL be mapped as Bevy maps such a Rust field. Each loaded entity SHALL get `TsSpawned`, so a restart despawns it like any TS-spawned entity. A saved `ChildOf` whose parent is not in the save, and `Children` entries not in it, SHALL be left out, so such an entity loads as a root. `parent: entity` SHALL make every loaded entity without a parent a child of `entity`. `TsSaved` and `TsSavedResources` SHALL be removed once their TS components and resources are written, except for entries kept under the next requirement.

#### Scenario: Save, despawn all, load, compare
- **WHEN** a world of `Save`-marked entities with Rust and TS components is saved, every one despawned, and the save loaded
- **THEN** the world has the same number of `Save`-marked entities, each with equal component values (compared by component, not by entity id)

#### Scenario: A TS Entity field is remapped
- **WHEN** entity `a` has the TS component `game::Follow { target: b }`, both are saved, despawned and loaded
- **THEN** the loaded `a`'s `target` is the loaded `b`, not the old id

#### Scenario: A hierarchy survives
- **WHEN** a parent and child, both with `Save`, are saved, despawned and loaded
- **THEN** the loaded child's `ChildOf` names the loaded parent

#### Scenario: A parent outside the save
- **WHEN** a `Save`-marked entity under an unmarked level entity is saved, despawned and loaded, once without and once with `parent: level`
- **THEN** it loads as a root whose `GlobalTransform` is its own `Transform`, then as the level's child

#### Scenario: Load adds
- **WHEN** a save of 3 entities is loaded twice with nothing despawned
- **THEN** the world has 6 `Save`-marked entities

### Requirement: TS components migrate on load and unknown ones are kept
On load, each `TsSaved` entry SHALL be written with `TsComponents::encode` when its name is declared in the live generation. Its fields SHALL migrate by the hot-reload rules: a field of the same name and type is kept, a new field takes its default, a field of another type is reset to its default, a field no longer declared is dropped; a component that migrated SHALL be reported once per load as a warning in the script log, naming the component and its field lists. An entry whose name is not declared SHALL stay in the entity's `TsSaved`, with one warning per name naming it, and SHALL be saved again by a later save of that entity; it SHALL never be dropped.

#### Scenario: A field was added since the save
- **WHEN** a save holds `game::Health { hp: 3 }` and `Health` now declares `hp` and `max: f32 = 10`
- **THEN** the loaded entity has `hp` 3 and `max` 10, and the log names `game::Health` with `max` added

#### Scenario: A class is gone
- **WHEN** a save holds `game::Shield` and no class is named `game::Shield` any more
- **THEN** the loaded entity keeps `game::Shield`'s JSON in `TsSaved`, the log warns naming `game::Shield`, and saving that entity again writes it back

### Requirement: The outcome is a message
bevy-ts SHALL add two Rust messages TS reads as it reads any Rust message: `WorldSaved { name, error }`, written once the file is renamed into place or the save failed, and `WorldLoaded { name, entities, error }`, written once the entities are in the world or the load failed, `entities` the new entities. `error` SHALL be absent on success and name the cause on failure: a missing file, an unreadable file (Bevy's error, with its position), a component Bevy cannot serialise (its type path), an IO error (the path).

#### Scenario: Loading a slot that does not exist
- **WHEN** a script calls `commands.loadWorld("nope")` and no such save exists
- **THEN** a `WorldLoaded` arrives with `name` `"nope"`, no entities and an `error` naming the missing file, and the world is unchanged

#### Scenario: The loaded entities
- **WHEN** a load of 3 entities succeeds
- **THEN** a `WorldLoaded` arrives holding those 3 entities

### Requirement: Saves live in the player's data folder
In a played game (a game folder with `game.json`), saves SHALL live in `<data>/<game name>/saves/`, `<game name>` the `name` in `game.json`, and `<data>` the per-user data folder: `~/Library/Application Support` on macOS, `%APPDATA%` on Windows, `$XDG_DATA_HOME` or else `~/.local/share` on Linux. Under `bevy-ts dev` and every engine started on a project folder, saves SHALL live in `<project>/.bevy-ts/saves/`. The folder SHALL be a resource, `SaveDir`, inserted when the app is built, which a test sets directly. The folder SHALL be created on the first save. Saves SHALL NOT be written into the game folder or the asset folder.

#### Scenario: A played game on macOS
- **WHEN** a game named `arena` is played on macOS and saves `slot1`
- **THEN** the file is `~/Library/Application Support/arena/saves/slot1.scn.ron`

#### Scenario: A dev engine
- **WHEN** `bevy-ts dev` runs the project at `P` and a script saves `slot1`
- **THEN** the file is `P/.bevy-ts/saves/slot1.scn.ron`

#### Scenario: Linux with XDG_DATA_HOME
- **WHEN** the folder is resolved on Linux with `XDG_DATA_HOME=/d` for a game named `arena`
- **THEN** it is `/d/arena/saves`

### Requirement: The coverage table names every Bevy item
docs/saves.md SHALL hold a table of every public item of Bevy 0.20's `bevy_world_serialization` that a save or load touches (`DynamicWorld`, `DynamicWorldBuilder` and each of its filter and extract methods, `WorldFilter`, `DynamicWorld::serialize`, `WorldDeserializer`, `write_to_world`, `DynamicWorldRoot`, `WorldInstanceReady`, `WorldInstanceSpawner`, `WorldAssetLoader`, handles in a save, `#[reflect(skip_serializing)]`), with its TS form, its status (covered, engine-only, open, or out of scope) and the test that shows it. `docs/research/bevy-coverage.md`'s `DynamicWorld` row SHALL name docs/saves.md and the tests.

#### Scenario: The matrix row
- **WHEN** this change lands
- **THEN** the "`DynamicWorld` save and load" row in `docs/research/bevy-coverage.md` says covered and links docs/saves.md
