# World saves

Research for `world-saves` (plan item 16, `bevy-coverage.md` "Assets and scenes"): saving and
loading a world from TS, the Bevy way. Read against Bevy 0.20.0 source
(`~/.cargo/registry/src/*/bevy_world_serialization-0.20.0`, `~/dev/refs/bevy-0.20`).

## Answer

- Bevy 0.20 calls it `DynamicWorld`, in `bevy_world_serialization` (re-exported as
  `bevy::world_serialization`). `DynamicScene` and `SceneSpawner` were renamed: `bevy_scene` now
  names the new BSN scene system (`components.rs:13-16`). bevy-ts already builds it with
  `serialize` on (`bevy-ts-tsx/Cargo.toml:22`; `ron` is in its `Cargo.lock` deps).
- Bevy's builder saves only types with a Rust `TypeId` in the `AppTypeRegistry`. TS components are
  dynamic components with no `TypeId` (`bevy-ts/src/components.rs:153-167`), so the builder skips
  every one of them without a word (`dynamic_world_builder.rs:315`, `.type_id()?`). TS resources
  too: they are TS components on a resource entity (`generation.rs:177-180`), skipped at `:389`.
- So "one builder, no hand-written serialiser" holds for Rust types and needs one addition for TS
  types: their JSON already has one owner, `value::decode_component` / `encode_component`
  (`value.rs:156`, `:171`). Nothing new is hand-written; a save file is two sections, each written
  by its owner.
- Saves live in a per-user data folder, not the asset folder: a shipped game's assets are
  read-only (an app bundle, Program Files, a store sandbox), and a save is per user. Bevy 0.20 has
  only `bevy_platform::dirs::preferences_dir` (`bevy_platform/src/dirs/`), made for
  `bevy_settings`; on macOS it is `~/Library/Preferences`, the plist folder, wrong for saves. No
  `dirs` crate is in the lock.

Sure: high on the Bevy API and the TS-component gap (read in source). Medium on the file format
and the folder (choices, below).

## Bevy 0.20, what exists

| Item | Where | What it does |
|---|---|---|
| `DynamicWorld { resources, entities }` | `dynamic_world.rs:25` | Reflected values, an `Asset` |
| `DynamicWorld::from_world(_with)` | `:59`, `:72` | Every entity (default query filters sidestepped, so disabled ones too) and every resource |
| `DynamicWorldBuilder::from_world(world, registry)` | `dynamic_world_builder.rs:90` | The builder |
| `allow_component::<T>` / `deny_component::<T>`, `_resource`, `allow_all`, `deny_all`, `allow_all_components` ... | `:119-222` | Typed filters |
| `with_component_filter(WorldFilter)`, `with_resource_filter` | `:103`, `:110` | Filters by `TypeId` (`WorldFilter::allow_by_id`, `world_filter.rs:88`, `:129`): the form TS can drive by name through the registry |
| `extract_entities(iter)`, `extract_entity`, `extract_resources`, `remove_empty_entities`, `build` | `:243-373`, `:232` | Extraction is immediate, with the filter as it stands then |
| Not extracted | `:305`, `:378` | Resource entities as entities; `DefaultQueryFilters` |
| `DynamicWorld::serialize(registry) -> String` (RON) | `dynamic_world.rs:229` | Through `DynamicWorldSerializer` (`serde.rs:60`), serde-generic |
| Handles in a save | `serde.rs:196-198` | Written as their asset path; a handle to an added (not loaded) asset is written as the default with a warning |
| `WorldDeserializer { type_registry, load_from_path }` | `serde.rs:221` | Reads it back; `AssetServer` is a `LoadFromPath` (`bevy_asset/src/reflect.rs:416`), so a save can be read outside the asset loader |
| `WorldAssetLoader` (`.scn`, `.scn.ron`) | `world_asset_loader.rs:50`, `:74` | Loads one from an asset source |
| `write_to_world(_with)(world, &mut EntityHashMap)` | `dynamic_world.rs:92`, `:211` | Spawns a fresh entity per saved one (`:105`), maps `Entity` fields through `SceneEntityMapper` (`MapEntities`, `#[entities]` as on `ChildOf`, `hierarchy.rs:107`), relationship hooks skipped (`:154`), resources last (`:198`) |
| `DynamicWorldRoot(Handle<DynamicWorld>)` | `components.rs:41` | Spawn as children of an entity, async; `WorldInstanceReady` fires |
| `WorldInstanceSpawner::spawn_dynamic(_sync)` | `world_asset_spawner.rs:164`, `:273` | Spawn by handle, keeps the entity map per instance |
| Registry needs | `dynamic_world.rs:124-134` | `#[reflect(Component)]` / `#[reflect(Resource)]`; field types need reflected serde or a reflected shape; `#[reflect(skip_serializing)]` leaves a field out (`examples/scene/world_serialization.rs`) |
| No `AssetSaver` for it | `dynamic_world.rs:226` (`TODO: move to AssetSaver`) | Bevy's example writes the file itself on the `IoTaskPool` |

## Coverage table draft

| Bevy item | TS form | Status | Test |
|---|---|---|---|
| `DynamicWorldBuilder` over chosen entities | `saveWorld({ entities: Query or With<Save> })` | missing | builder round-trip, `bevy-ts` |
| `allow_component` / `deny_component` | `allow: [Health, Transform]`, `deny: [...]` by component class | missing | filter test |
| `allow_resource` / `deny_resource` | `resources: { allow, deny }` | missing | filter test |
| Rust reflected components and resources | through the builder | missing | Transform + a Rust resource round-trip |
| TS components and resources | second section via `value.rs` | missing (Bevy skips them) | TS component round-trip |
| `Entity` fields remapped on load | Bevy's map for Rust fields; same `EntityHashMap` for TS `Entity` fields (`value.rs:76`, `to_bits`) | missing | a TS `target: Entity` survives load |
| Handles saved as paths | as is | missing | a mesh handle round-trip |
| `DynamicWorld::serialize` / `WorldDeserializer` | none; the engine does it | missing | |
| `write_to_world` | `loadWorld(name)` | missing | |
| `DynamicWorldRoot`, `WorldInstanceReady` | `loadWorld(name, { parent })`, then the existing `WorldInstanceReady` event | missing | |
| Save folder | `saveWorld("slot1")` resolves under the user data folder | missing | path test with a temp override |
| `.scn.ron` as an asset (`WorldAssetLoader`) | TSX `DynamicWorldRoot` path | open (levels are `.bsn`) | |
| `#[reflect(skip_serializing)]` | none for TS fields yet | open | |

## In this repo

- TS components: `ComponentDescriptor::new_with_layout`, plain bytes, `ComponentCloneBehavior::Default`
  (`components.rs:153-167`). Layout and decl per name in `TsComponents` (`:63`). Their JSON is
  `value::decode_component` / `encode_component`; `Entity` fields are `to_bits` numbers
  (`value.rs:141`).
- Rust components TS sees: reflect tier and layout tier both have a `TypeId` and, when they
  derive `#[reflect(Component)]`, the builder covers them. A native `#[export]` component without
  `#[reflect(Component)]` is not saved: say so in the docs.
- Reflected JSON (`docs/reflection.md`) is the inspector's shape, not the save's: the save of Rust
  types is Bevy's own RON through the registry, so a Bevy tool can read it.
- TS reaches the world through command ops applied with `&mut World` (`commands.rs:55-72`); save
  and load are two new ops (or one host call). They run exclusive, which `write_to_world` needs.
- Generations: restart despawns `TsSpawned` entities and TS resources (`generation.rs:165-187`).
  A loaded entity should get `TsSpawned` too, so a restart clears it like any spawned one.
- Assets and IO: `.bsn` levels are written by bevy-ts's own writer into the project
  (`docs/levels.md`), which is the dev source tree. Saves are not assets and do not go there.

## Recommended design

One TS call each way, one builder underneath:

- `commands.saveWorld(name, { entities, allow, deny, resources })`: an exclusive op builds a
  `DynamicWorldBuilder` with `WorldFilter`s made from the named classes (Rust ones by `TypeId`
  from the registry), and, for the same entities, collects TS components through `TsComponents`
  + `value::decode_component`. It writes `{ bevy: <DynamicWorldSerializer RON>, ts: { version,
  entities: { <saved entity bits>: { <component name>: <fields> } }, resources } }`, on the
  `IoTaskPool`, to a temp file then renamed.
- `commands.loadWorld(name)`: reads with `WorldDeserializer` (load_from_path = `AssetServer`),
  `write_to_world` with a fresh `EntityHashMap`, then writes TS components with
  `TsComponents::encode` on the mapped entities, mapping `Entity` fields through the same map.
  Unknown TS names or fields go through the existing migration rules (`components.rs`, kept /
  added / reset / removed), so an old save loads into new code.
- The folder: `<user data dir>/<app id>/saves/<name>.save.ron`, where the data dir is macOS
  `~/Library/Application Support`, Windows `%APPDATA%`, Linux `$XDG_DATA_HOME` or
  `~/.local/share`; in dev, a `saves/` folder in the project, ignored by git. About 20 lines in the
  style of `bevy_platform::dirs`; no new crate.

Rejected: Bevy's builder alone (drops every TS component); a hand-written serialiser for Rust
types (duplicates the registry); `preferences_dir` (wrong macOS folder); the asset folder
(read-only when shipped).

## Open questions

1. The app id for the folder: from `bevy-ts.json`, or the same `app_name` a `SettingsPlugin`
   takes, so settings and saves sit together?
2. Which entities by default: everything `TsSpawned`, or only those with a `Save` marker (the
   replicon way, `Replicated`)? A marker is safer: level entities come back from the level.
3. Load replaces or adds: despawn the previous saved set first, or leave that to the game?
4. Web: Bevy's `bevy_settings` uses `localStorage` on wasm; does bevy-ts ship web builds yet?
5. A save of a component whose TS class is gone: keep it as JSON and warn, or drop it?
