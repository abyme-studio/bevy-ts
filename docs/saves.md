# World saves

A script saves the world to a file and loads it back, the Bevy way: Bevy's `DynamicWorldBuilder`
builds the save, `DynamicWorld::serialize` writes Bevy's own `.scn.ron`, and Bevy's
`WorldDeserializer` reads it. TS components ride in the same file. Owner: `bevy_ts::saves`.

```ts
import { Component, f32, Save, With } from "bevy-ts";
import type { Commands, Entity, MessageReader, Query } from "bevy-ts";
import { WorldLoaded, WorldSaved } from "bevy-ts/bevy";

export class Health extends Component { static path = "game::Health"; declare hp: f32 }

export function spawn(c: Commands) {
  c.spawn(Save, Health.of({ hp: 3 }));
}

export function saveGame(c: Commands) {
  c.saveWorld("slot1");
}

// Load replaces only when the game says so: despawn the saved set first.
export function loadGame(c: Commands, saved: Query<[Entity], With<Save>>) {
  saved.each((e) => c.entity(e).despawn());
  c.loadWorld("slot1");
}

export function outcomes(saved: MessageReader<WorldSaved>, loaded: MessageReader<WorldLoaded>) {
  for (const m of saved) console.log(m.error ?? `saved ${m.name}`);
  for (const m of loaded) console.log(m.error ?? `loaded ${m.entities.length} entities`);
}
```

## The calls

- `commands.saveWorld(name, { entities, allow, deny, resources })` and
  `commands.loadWorld(name, { parent })`. Both are command ops: they run when the buffer is
  applied, and return nothing.
- `name` is 1 to 64 of `A-Z a-z 0-9 _ -`. Anything else (`"../x"`, `"a/b"`, `""`) throws in TS
  and is refused again in Rust, so a save never leaves its folder.
- The file is `<name>.scn.ron`. Each save writes its own `<name>.scn.ron.<n>.tmp` in the same
  folder, syncs it, then renames it over the old one: a crash or a failed write leaves the old save
  whole, and two saves of one name at once leave one of them whole (the last rename).
- The outcome is a message. `WorldSaved { name, error }` comes once the file is in place or the
  save failed; `WorldLoaded { name, entities, error }` once the entities are in the world, with
  the new entities. `error` is `None` on success (`null` in TS) and names the cause: a missing file, Bevy's parse
  error with its position, a component with no serde form (its type path), an IO error (the
  path). A script reads them with `MessageReader<WorldSaved>` (both from `bevy-ts/bevy`);
  Rust reads them as any message.

## What a save holds

- By default, every entity with the `Save` marker, disabled ones included. Resource entities are
  never saved as entities, even when `entities` names one. `Save` is a Rust
  component, saved itself, so a loaded entity has it again. Level entities come back from their
  level, so they are not marked.
- `entities: query` (every entity it matches) or `entities: [a, b]` replaces that set.
- Every reflected Rust component (`#[reflect(Component)]`) on those entities, written by Bevy's
  serializer. Handles are written as their asset path and load through the `AssetServer`.
- Every TS component, as one reflected `bevy_ts::saves::TsSaved` per entity: TS component name to
  its JSON (`value::decode_component`). An entity with no TS component has none.
- `allow: [Health, Transform]` saves only those (and `Save`); `deny: [...]` leaves them out, after
  `allow`. Rust classes by their type path, TS classes by name. A class that is neither fails the
  save, naming it.
- No resources, unless `resources: [ClearColor, Score]` names them, Rust or TS. TS resources ride in
  one reflected `TsSavedResources`.

Never saved:

- A Rust component without `#[reflect(Component)]`: a native `#[export]` component is saved only
  with it (docs/native.md).
- bevy-ts's bookkeeping (`TsSpawned`, `ScriptHidden`).
- What Bevy computes from other components, which comes back by itself: `GlobalTransform`,
  `TransformTreeChanged`, `InheritedVisibility`, `ViewVisibility`, visible entity lists, frusta,
  cascades, and UI and text layout (`ComputedNode`, `UiGlobalTransform`, `ContentSize`,
  `TextLayoutInfo`, ...). `ObservedBy`, which names observer entities a save never holds.
- Components with no serde form in Bevy 0.20, which would fail every save that met them:
  `VisibilityClass`, `CameraMainTextureUsages`, `CameraRenderGraph`, `Exposure`, `ColorGrading`. A
  saved camera loads with their defaults.

The list is `saves::ALWAYS_DENIED`. It was measured over every entity of `examples/arena`
(openspec world-saves, task 3.2); `crates/bevy-ts-cli/tests/it/saves.rs`
`the_whole_arena_saves` saves all of them on a rendering engine and checks each listed path is a
registered type, so a Bevy upgrade that adds an unserialisable component fails there, naming it.

## Load

- Load adds: each saved entity becomes a new entity. The live world is left as it is; a game that
  replaces its state despawns its `Save`-marked entities first (above). Loading a save twice gives
  two copies.
- Entity references are remapped. Rust `Entity` fields by Bevy (`ChildOf`, `MapEntities` types);
  TS `Entity` fields through the same map. A reference to an entity outside the save maps as Bevy
  maps one in a Rust field: to an entity that is never alive.
- Each loaded entity gets `TsSpawned`, so a restart despawns it like any spawned entity.
- The hierarchy is cut at the save's edge: a `ChildOf` whose parent the save does not hold is
  left out, so an enemy saved without its level loads as a root; a saved parent's `Children`
  keep only the saved children.
- `parent: e` makes each loaded entity that has no parent a child of `e`, such as the level the
  enemies were under.
- A load reads the file as it is when the load starts: a save of that name still writing is not
  in it.
- Saved Rust resources overwrite the live ones; saved TS resources are inserted or overwritten.

## Old saves in new code

TS fields migrate by the rules a reload uses (`components::commit`, openspec `hot-reload`): a field of the same name and type is
kept, a new field takes its default, a field whose type changed is reset to its default, a field
no longer declared is dropped. The type is read from the saved value's shape (a float, an
integer, a list of its length, an enum's name...). Each component that migrated is logged once per
load as a warning: `load: game::Health migrated on 3 entities; added max; reset hp`.

A TS component whose class no longer exists is never dropped: its JSON stays in the entity's
`TsSaved`, the log warns once per name, and a later save of that entity writes it again. When the
class comes back, the next load turns it into the component. Its `Entity` fields are not remapped
while it waits, since nothing declares which fields they are.
A TS resource whose class is gone is kept the same way, in `TsSavedResources`, and written by
every later save.

Rust types follow Bevy's rules: a field Bevy cannot read fails the load with Bevy's error.

## The folder

| Where | Folder |
|---|---|
| `bevy-ts dev` on a project | `<project>/.bevy-ts/saves/`, already git-ignored |
| A played game on macOS | `~/Library/Application Support/<game>/saves/` |
| on Windows | `%APPDATA%\<game>\saves\` |
| on Linux | `$XDG_DATA_HOME/<game>/saves/`, else `~/.local/share/<game>/saves/` |

`<game>` is the `name` in `game.json`. The folder is a resource, `SaveDir`, set when the engine
is built (`cli::dev_save_dir`, `play::save_dir`) and created on the first save. Saves are never
written into the game folder or the asset folder: a shipped game's are read-only, and a save is
the player's. A test sets `SaveDir` itself. Not `bevy_platform::dirs::preferences_dir`: on macOS
that is `~/Library/Preferences`, the plist folder.

Web builds are out of scope: bevy-ts ships none. When it does, saves follow Bevy's
`bevy_settings` into `localStorage`.

## Cost

The world walk and the RON run on the main thread, in the op; the file write, and a load's read
and parse, run on the `IoTaskPool`; the write into the world runs in one frame. Nothing runs per
frame otherwise. `bench/world-saves` (`run.sh`, release, Bevy static), 10,000 `Save`-marked
entities, each with `Transform` and two TS components:

| Run | Save, main thread | Save, end to end | File | Load, one frame | Load, end to end |
|---|---|---|---|---|---|
| 1 | 146.5 ms | 381.1 ms | 4606 KB | 225.0 ms | 505.6 ms |
| 2 | 120.8 ms | 291.7 ms | 4606 KB | 214.5 ms | 467.6 ms |
| 3 | 184.0 ms | 295.8 ms | 4606 KB | 136.9 ms | 223.9 ms |

Measured on an M-series Mac under a load average of 130 (other builds running), so read the
spread as noise and the medians as an upper bound. Either way, a save or a load of 10k entities
stalls a frame by 120 to 225 ms, about 8 to 14 frames at 60 Hz: save and load at a pause, a
menu or a level change, not mid-play. The cost is linear in what is saved; a few hundred
entities fit in a frame.

## Coverage

Every public item of Bevy 0.20's `bevy_world_serialization` a save or load touches. Tests are in
`crates/bevy-ts/tests/it/saves.rs` unless a file is named; `cli` is
`crates/bevy-ts-cli/tests/it/saves.rs`.

| Bevy item | TS form | Status | Test |
|---|---|---|---|
| `DynamicWorld { resources, entities }` | a save file | covered | `save_despawn_all_load_compare` |
| `DynamicWorld::from_world`, `from_world_with` | none: every entity and resource is never what a game saves | engine-only | — |
| `DynamicWorldBuilder::from_world` | `saveWorld` | covered | `save_despawn_all_load_compare` |
| `with_component_filter(WorldFilter)` | `allow`, `deny` (Rust classes) | covered | `the_saved_set_and_the_filters` |
| `with_resource_filter(WorldFilter)` | `resources` | covered | `save_despawn_all_load_compare` |
| `allow_component::<T>`, `deny_component::<T>` | `allow: [T]`, `deny: [T]` | covered (by id) | `the_saved_set_and_the_filters` |
| `allow_all_components`, `deny_all_components` | no `allow`; `allow: []` | covered | `the_saved_set_and_the_filters` |
| `allow_resource::<T>`, `deny_resource::<T>` | `resources: [T]` | covered (allow only: none is saved unless named) | `save_despawn_all_load_compare` |
| `allow_all_resources`, `deny_all_resources` | none: resources are opt-in | out of scope (most are Bevy's own and not serialisable) | — |
| `allow_all`, `deny_all` | none: the two above | out of scope | — |
| `WorldFilter::allow_by_id`, `deny_by_id` | how `allow`, `deny`, `resources` reach the builder | covered | `the_saved_set_and_the_filters` |
| `extract_entities(iter)`, `extract_entity` | `entities: query` or `[a, b]`; `Save` by default | covered | `the_saved_set_and_the_filters` |
| `extract_resources` | `resources: [..]` | covered | `save_despawn_all_load_compare` |
| `remove_empty_entities` | none: a saved entity keeps `Save` | out of scope | — |
| `build` | `saveWorld` | covered | `save_despawn_all_load_compare` |
| Reflected Rust components and resources | through the builder | covered | `save_despawn_all_load_compare` (`Transform`, `Name`, `ChildOf`, a resource) |
| TS components and resources | `TsSaved`, `TsSavedResources` | covered | `save_despawn_all_load_compare`, cli `a_script_saves_and_loads_in_the_dev_folder` |
| `DynamicWorld::serialize` (RON) | the file, `<name>.scn.ron` | covered | `bevy_reads_a_rust_only_save_and_handles_are_paths` |
| Handles in a save (`HandleSerializeProcessor`) | written as their asset path | covered | `bevy_reads_a_rust_only_save_and_handles_are_paths` |
| `WorldDeserializer { type_registry, load_from_path }` | `loadWorld` (`AssetServer` loads handles) | covered | `save_despawn_all_load_compare` |
| `write_to_world`, `write_to_world_with` | `loadWorld`: fresh entities, load adds | covered | `save_despawn_all_load_compare` |
| `Entity` fields remapped (`SceneEntityMapper`) | Rust fields by Bevy, TS fields by the same map | covered | `save_despawn_all_load_compare`; `saves::tests::a_high_generation_entity_maps_exactly` |
| `ChildOf`, `Children` at the save's edge | cut: an unsaved parent or child is left out | covered | `a_parent_outside_the_save_is_cut` |
| `WorldAssetLoader` (`.scn`, `.scn.ron`) | none: a save is not an asset; Bevy reads a bevy-ts save | covered (Bevy reads ours) | `bevy_reads_a_rust_only_save_and_handles_are_paths` |
| `DynamicWorldRoot(Handle<DynamicWorld>)` | `loadWorld(name, { parent })` | covered (parent); as an asset from TSX: out of scope, levels are `.bsn` | `a_load_under_a_parent` |
| `WorldInstanceReady` | `WorldLoaded` | covered | `save_despawn_all_load_compare` |
| `WorldInstanceSpawner::spawn_dynamic(_sync)` | none: `loadWorld` writes the world once read | engine-only | — |
| `#[reflect(skip_serializing)]` | Rust types: as Bevy; TS fields: none | open | — |
| The save folder | `SaveDir`: dev and per-user data folder | covered | `saves::tests::the_data_folder_per_os`; cli `a_script_saves_and_loads_in_the_dev_folder`, `a_played_game_saves_in_the_players_data_folder` |
| Save names | `[A-Za-z0-9_-]{1,64}` | covered | `a_name_with_a_path_writes_nothing`; `saves::tests::a_name_cannot_leave_the_folder`; cli `a_script_saves_and_loads_in_the_dev_folder` |
| Temporary file then rename | the write, one temporary file per save | covered | `a_missing_slot_and_a_failed_write`, `two_saves_of_one_slot_at_once` |
| Migration and kept classes | hot-reload rules on load | covered | `old_saves_migrate_and_unknown_classes_stay` |
| Web (`localStorage`) | none | out of scope (no web builds) | — |
