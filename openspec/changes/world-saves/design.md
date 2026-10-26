## Context

Research: docs/research/world-saves.md (d9ff71ca), read against Bevy 0.20 source. Bevy 0.20 calls a
saveable world `DynamicWorld` (`bevy_world_serialization`); its builder saves only types with a
`TypeId` in the `AppTypeRegistry` (`dynamic_world_builder.rs:315`, `:389`). TS components are
dynamic (`crates/bevy-ts/src/components.rs:153-167`), so Bevy alone saves none of them. Their
JSON has one owner already: `value::decode_component` / `encode_component` (`value.rs:156`, `:171`).

Every engine already builds `bevy_world_serialization` with `serialize`: `bevy-ts-cli` takes Bevy's
`3d`, which takes `scene`. `ron` 0.12 is in the lock through it.

TS reaches the world through command ops applied with `&mut World` (`commands.rs:55-72`).
Restart despawns `TsSpawned` entities and TS resources (`generation.rs:165-187`).

## Goals / Non-Goals

**Goals:**
- One call to save and one to load, from TS, the Bevy way: Bevy's builder, Bevy's file.
- TS components and resources saved and loaded, `Entity` fields remapped, old saves loading into
  new code.
- A save folder right for a shipped game on each desktop OS.

**Non-Goals:**
- Web builds: bevy-ts ships none. When it does, saves follow Bevy's `bevy_settings` and use
  `localStorage`; nothing here blocks that.
- A save as an asset (`DynamicWorldRoot` from TSX, `.scn.ron` in `assets/`): levels are `.bsn`.
- `#[reflect(skip_serializing)]` for TS fields.
- A `SettingsPlugin` or settings file.
- Save metadata (slot list, thumbnails, play time). A game keeps those in its own `Save`-marked
  entity or resource.

## Decisions

1. Save marker by default (lead's decision 2). Only `Save`-marked entities are saved unless
   `entities` says otherwise. Level entities come back from the level; saving them too would load
   them twice. Alternative: everything `TsSpawned`; rejected, it saves the level's spawns and the
   UI. `Save` is a Rust component, reflected, so the builder saves it with no special case.

2. Load adds (lead's decision 3). `loadWorld` spawns; it never despawns. A game that replaces its
   state runs `for (const e of query(Entity).with(Save)) commands.despawn(e)` first; docs/saves.md
   shows it. Alternative: a `replace` option; rejected for now, since what "the previous set" is
   (Save-marked, or what the last load spawned) is the game's call.

3. TS data rides inside Bevy's `DynamicWorld`, not beside it. The research proposed a file of two
   sections. Instead, after `build()`, the op pushes one `TsSaved` (a reflected
   `BTreeMap<String, String>`, name to JSON text) onto each `DynamicEntity.components`, and one
   `TsSavedResources` onto `DynamicWorld.resources`. Both fields are public. Then the file is
   exactly `DynamicWorld::serialize`, `<name>.scn.ron`, which Bevy's `WorldAssetLoader` reads, and
   there is no outer format or deserialiser to write. `TsSaved` has no `MapEntities`; the load
   remaps TS `Entity` fields itself with the same `EntityHashMap` `write_to_world` filled.
   Alternative: two sections in one file (research); rejected, it needs an outer `DeserializeSeed`
   wrapping `WorldDeserializer`, and Bevy cannot read the file.

4. Unknown TS classes are kept (lead's decision 5). An entry whose name is not declared stays in
   the entity's `TsSaved`. A later save merges that entity's live TS components over its leftover
   entries, so a class that comes back later finds its data. Alternative: drop with a warning;
   rejected, a save would lose data on a code change it did not cause.

5. Migration reuses the hot-reload rules. `components::plan` (`components.rs:175`, made `pub(crate)`)
   already says kept / added / reset / removed between two layouts. The load builds the saved
   layout from the JSON's keys and value shapes, runs the same plan, and logs the `Migration`
   as reload does. Unknown fields are dropped (they are "removed"); `encode_component` would
   otherwise refuse them.

6. TS `Entity` fields remap as Bevy remaps. A field naming an entity in the save gets the new
   entity from the map. One naming an entity outside the save goes through the same
   `EntityMapper` Bevy uses for Rust fields (`SceneEntityMapper`), so a Rust and a TS reference to
   the same missing entity end the same way.

7. Filters by class. `allow` / `deny` take classes. A Rust class has a reflect `path`; the op looks
   up its `TypeId` in the registry and builds `WorldFilter::allow_by_id` / `deny_by_id`
   (`world_filter.rs:88`, `:129`) for `with_component_filter`. A TS class filters `TsSaved`'s
   entries by name. Resources are opt-in only (`resources: [..]`): `DynamicWorld::from_world`
   saves every resource, most of them Bevy's own and many not serialisable. bevy-ts's own
   bookkeeping components (`TsSpawned`, `ScriptHidden`, the internal ones) are always denied.

8. Async the Bevy way. The world walk and `serialize` run in the op (they need `&World`); the
   write runs on `IoTaskPool`, temp file then `rename`, as Bevy's own example writes on the pool.
   The load reads and deserialises on the pool (the registry is an `Arc`, the `AssetServer` a
   clone), then an exclusive system applies the finished `DynamicWorld` on a later frame.

9. The folder (lead's folder decision, decision 1). `SaveDir(PathBuf)` is a resource.
   - Played game: `<data>/<name>/saves`, `<name>` from `game.json`. bevy-ts has no
     `SettingsPlugin` today; when one is added, it takes the same name so settings and saves sit
     together (decision 1).
   - `bevy-ts dev` or any engine on a project folder: `<project>/.bevy-ts/saves`. `.bevy-ts/` is
     already git-ignored and per project, so no new ignore line. The lead asked for "a git-ignored
     `saves/` in the project"; this is that, one folder down.
   - `<data>` comes from a pure function `data_dir(os, env, home)` in about 20 lines in the style
     of `bevy_platform::dirs`. It takes the environment as a lookup, so a test passes
     `XDG_DATA_HOME` without setting a variable (docs/testing.md). No `dirs` crate.
   - Not `bevy_platform::dirs::preferences_dir`: on macOS that is `~/Library/Preferences`, the
     plist folder.

10. Results as Rust messages. `WorldSaved` and `WorldLoaded` are reflected Bevy messages,
    registered for TS as any Rust message (`ReflectTsMessage`, `rust_events.rs`). A script reads
    them with a `MessageReader` param. Alternative: a promise from `saveWorld`; rejected, commands
    return nothing elsewhere and a system cannot await.

## Risks / Trade-offs

- [A reflected Bevy component on a saved entity has no serde form; `serialize` fails the whole
  save] → the error names the type; the walk test lists which components of the arena example
  fail, and those computed or render-only ones (`GlobalTransform`, `InheritedVisibility`,
  `ViewVisibility`, mesh-bounds caches) go in the always-denied list. Task 3.2 records the list.
- [Numbers in JSON inside RON lose `u64` precision] → `Entity` bits are written by
  `serde_json` as integers, not floats; the remap test checks a high-generation entity.
- [A big save stalls the frame] → the walk and RON run on the main thread. The bench measures
  save and load at 10k entities; if a save takes more than one 60 Hz frame (16 ms), say so in
  docs/saves.md and in the report rather than splitting the walk.
- [A save file is written to the player's own folder] → only on a script's `saveWorld`, only
  under the game's own name.

## Migration Plan

Additive: no existing call or file changes. Rollback is removing the two ops.

## Measured (task 3.2)

A save of all 155 entities of `examples/arena` on a rendering engine, no deny list. Serialising each
reflected component alone, five fail with no serde form in Bevy 0.20: `VisibilityClass`
(`TypeId`), `CameraMainTextureUsages`, `CameraRenderGraph`, `Exposure`, `ColorGrading`
(`Range<f32>`). Handles (`Mesh3d`, `MeshMaterial3d`, `RenderTarget`) fail alone but not in a
save: the save's serializer writes them as paths. Computed and denied too: `GlobalTransform`,
`TransformTreeChanged`, `InheritedVisibility`, `ViewVisibility`, `VisibleEntities`,
`CascadesVisibleEntities`, `Frustum`, `CascadesFrusta`, `Cascades`, the UI and text layout
components, and `ObservedBy` (it names observer entities). With that list
(`saves::ALWAYS_DENIED`) the whole arena saves; `the_whole_arena_saves` (bevy-ts-cli) keeps it so.
`Exposure` and `ColorGrading` are user data: a saved camera loads with their defaults
(docs/saves.md says so).

## As built

- Migration (decision 5) compares each saved value's JSON shape with the live field type's (a
  float, an integer, a bool, a string, a list of its length, an object of its keys) instead of
  building a layout from the JSON and calling `components::plan`: JSON cannot tell `u32` from
  `i32`, but the shape tells a retyped field, which is what the rules need. It fills the same
  `Migration` and logs it as reload does.
- `Save` is a unit struct, which reflection gives no layout, so TS could not spawn it.
  `saves::register` gives it an empty native layout (as `ts_layout` does), so `spawn(Save, ...)`
  works.
- The dev folder is set in `run_dev`, which knows the project, not `base_app`, which does not.
- The two messages are registered with rust-events' `register_message`, so `bevy-ts/bevy`
  generates `WorldSaved` and `WorldLoaded` and a script reads them with `MessageReader`.

## Measured (task 5.3)

`bench/world-saves`, release, 10k `Save`-marked entities each with `Transform` and two TS
components, 3 runs under a load average of 130 (the arms are one, so no second arm to be within
noise of):

| | run 1 | run 2 | run 3 |
|---|---|---|---|
| save, main thread (walk and RON) | 146.5 ms | 120.8 ms | 184.0 ms |
| save, end to end (with the write) | 381.1 ms | 291.7 ms | 295.8 ms |
| file | 4606 KB | 4606 KB | 4606 KB |
| load, the one frame that writes the world | 225.0 ms | 214.5 ms | 136.9 ms |
| load, end to end (with read and parse) | 505.6 ms | 467.6 ms | 223.9 ms |

About 13 to 22 µs an entity on the main thread: a save or load of 10k stalls 8 to 14 frames, so
games save at a pause; up to about a thousand entities fit a 16 ms frame.

## Open Questions

None blocking.
