All cargo runs go through `~/dev/abyme/scripts/gate-slot`, headless. New tests are modules of their
crate's one test binary (`crates/<crate>/tests/it/<name>.rs` plus a `mod` line in `main.rs`), per
docs/testing.md: ports from `common::free_port()`, folders from `common::project`, no environment
variables, waits on conditions with deadlines.

## 1. Types and the folder

- [ ] 1.1 `crates/bevy-ts/Cargo.toml`: `bevy/bevy_world_serialization` on the `bevy` dependency; `ron` (0.12, already in the lock) as a direct dependency only if `WorldDeserializer` needs a `ron::Deserializer` built here
- [ ] 1.2 `crates/bevy-ts/src/saves.rs` (new): `Save` (reflected marker), `TsSaved(BTreeMap<String, String>)` and `TsSavedResources` (reflected component and resource), `WorldSaved { name, error }` and `WorldLoaded { name, entities, error }` (reflected messages), `SaveDir(PathBuf)` resource; register all in `crates/bevy-ts/src/lib.rs` beside `TsSpawned`; `WorldSaved` and `WorldLoaded` registered for TS with `ReflectTsMessage` (`rust_events.rs`)
- [ ] 1.3 `saves.rs`: `data_dir(os, env: impl Fn(&str) -> Option<String>, home)` for macOS, Windows and Linux (design decision 9), and `check_name` for `[A-Za-z0-9_-]{1,64}`; unit tests in `saves.rs` for each OS, `XDG_DATA_HOME` set and unset, and bad names (`..`, `/`, empty, 65 chars)
- [ ] 1.4 `crates/bevy-ts-cli/src/cli.rs` `base_app`: insert `SaveDir(<project>/.bevy-ts/saves)`; `crates/bevy-ts-cli/src/play.rs` `app`: insert `SaveDir(data_dir(..)/<game.json name>/saves)`

## 2. Save

- [ ] 2.1 `crates/bevy-ts/src/commands.rs`: `OP_SAVE_WORLD` (14) and `OP_LOAD_WORLD` (15), `Op::SaveWorld { name, options }` and `Op::LoadWorld { name, parent }`, decoded and applied through `saves.rs`; decode round-trip tests beside the existing ones
- [ ] 2.2 `saves.rs` `save`: the saved set (`Save`-marked, or the given query or entities; resource entities never); `DynamicWorldBuilder` with `WorldFilter`s from Rust classes by registry `TypeId`, plus the always-denied list (`TsSpawned`, `ScriptHidden`, bevy-ts internals, and 3.2's list); `extract_entities`; with `resources`, `extract_resources` under a resource filter; then push `TsSaved` per entity (TS components through `TsComponents` and `value::decode_component`, filtered by TS class, merged over any leftover `TsSaved`) and `TsSavedResources`; `serialize`; write on `IoTaskPool` to `<name>.scn.ron.tmp` then rename; `WorldSaved` on finish or failure
- [ ] 2.3 A class neither a registered Rust component nor a declared TS component fails the op with `WorldSaved.error` naming it

## 3. Load

- [ ] 3.1 `saves.rs` `load`: read and `WorldDeserializer` (load_from_path the `AssetServer`) on `IoTaskPool`; an exclusive system takes the finished `DynamicWorld` and runs `write_to_world` with a fresh `EntityHashMap`; adds `TsSpawned` to each new entity; `parent` adds `ChildOf` to each new root; writes each declared `TsSaved` entry with `TsComponents::encode` after migration (`components::plan`, made `pub(crate)`), TS `Entity` fields mapped through the same map and Bevy's `SceneEntityMapper` for ones outside it; leaves undeclared entries in `TsSaved` with one warning per name; logs migrations; writes TS resources from `TsSavedResources`; removes the emptied `TsSaved` / `TsSavedResources`; `WorldLoaded` with the new entities or the error
- [ ] 3.2 Run a save over every entity of `examples/arena` with no deny list; record in design.md which reflected components fail `serialize`; add the computed and render-only ones to the always-denied list in `saves.rs`

## 4. TS API

- [ ] 4.1 `crates/bevy-ts-v8/js/core.js`: `commands.saveWorld(name, { entities, allow, deny, resources })` and `commands.loadWorld(name, { parent })`, the name checked in JS with the same rule, classes sent as their name or reflect path; regenerate `packages/bevy-ts/runtime/core.js` with `packages/bevy-ts/scripts/sync-runtime.mjs`
- [ ] 4.2 `packages/bevy-ts/index.d.ts`: the two methods on `Commands` with their option types; `Save`, `WorldSaved`, `WorldLoaded` declared and exported

## 5. Tests

- [ ] 5.1 `crates/bevy-ts/tests/it/saves.rs` (new, `mod saves;` in `main.rs`), an app with `MinimalPlugins`, `AssetPlugin` and `SaveDir` in the test's `common::project` folder, TS components registered through `TsComponents`: save, despawn all, load, compare (Rust `Transform`, a Rust resource, TS components, a TS resource; compared by component per `Name`, never by entity id); a TS `target: Entity` remapped, including a high-generation entity and one outside the save; `ChildOf` remapped; `Save` default set vs `entities`; `allow` and `deny` for Rust and TS classes; `resources`; load adds (twice gives double); an added field defaults and a retyped one resets, with the migration logged; an undeclared class kept in `TsSaved` and saved again; a missing slot gives `WorldLoaded.error`; a failed write (the temp path a folder) keeps the old file whole; a Rust-only save read by Bevy's `WorldAssetLoader`; a mesh handle saved as its path and loaded
- [ ] 5.2 `crates/bevy-ts-cli/tests/it/saves.rs` (new, `mod saves;` in `main.rs`): one headless engine on a project, a script that spawns `Save`-marked entities, saves, despawns, loads, and reads `WorldSaved` / `WorldLoaded`; asserts the file is under `<project>/.bevy-ts/saves/` and the reloaded values match; `saveWorld("../x")` throws in TS
- [ ] 5.3 `bench/world-saves` (new, outside the workspace): save and load of 10k `Save`-marked entities each with `Transform` and two TS components, `cargo static test --release`, 3 runs; record main-thread ms for save (walk plus serialise), file size, and load (apply) in design.md and docs/saves.md

## 6. Docs

- [ ] 6.1 `docs/saves.md` (new): the API, the `Save` marker, load adds (with the despawn-first snippet), filters, the folder per OS and in dev, migration and kept unknown classes, what is not saved (Rust components without `#[reflect(Component)]`), web out of scope, the bench numbers, and the coverage table: every Bevy 0.20 `bevy_world_serialization` item from docs/research/world-saves.md with its TS form, status and test
- [ ] 6.2 `docs/research/bevy-coverage.md:110`: the `DynamicWorld` row says covered, links docs/saves.md and names `crates/bevy-ts/tests/it/saves.rs`; plan item 16 marked done
- [ ] 6.3 `docs/native.md`: one line that a native `#[export]` component is saved only with `#[reflect(Component)]`
