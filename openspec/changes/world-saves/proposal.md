## Why

A game cannot save from TS: there is no call, and Bevy's own `DynamicWorldBuilder` silently skips
every TS component and TS resource, since they have no `TypeId` (docs/research/world-saves.md).
The coverage matrix lists `DynamicWorld` save and load as missing (`docs/research/bevy-coverage.md`,
"Assets and scenes", plan item 16).

## What Changes

- `commands.saveWorld(name, options?)` and `commands.loadWorld(name, options?)`, two command ops
  run with `&mut World`.
- One builder: Bevy's `DynamicWorldBuilder` saves the Rust reflected components and resources.
  TS components ride in the same `DynamicWorld` as one reflected component per entity,
  `TsSaved`, written by the existing owner of TS JSON (`value::decode_component`). No serialiser
  is written for Rust types. The file is Bevy's own RON, `<name>.scn.ron`.
- A `Save` marker component. By default a save holds only entities with `Save`; `entities: query`
  overrides. Level entities come back from the level, not the save.
- Filters by class: `allow: [..]`, `deny: [..]` for components (Rust and TS); `resources: [..]`
  names the resources to save (none by default).
- Load adds: every saved entity is spawned fresh, `Entity` fields remapped (Bevy's map for Rust
  fields, the same map for TS `Entity` fields), and marked `TsSpawned` so a restart clears it.
  The game despawns its old saved set itself.
- TS fields migrate on load by the existing rules (kept, added, reset, removed). A TS component
  whose class is gone stays on the entity as JSON in `TsSaved`, with a warning, and is saved
  again; it is never dropped.
- Two Rust messages TS reads: `WorldSaved { name, error }` and `WorldLoaded { name, entities,
  error }`. File IO runs on Bevy's `IoTaskPool`; a save writes a temp file and renames it.
- The save folder: in a played game, the per-user data folder (macOS
  `~/Library/Application Support`, Windows `%APPDATA%`, Linux `$XDG_DATA_HOME` or
  `~/.local/share`) under the game's name, then `saves/`. Under `bevy-ts dev`,
  `<project>/.bevy-ts/saves/`, already git-ignored. About 20 lines, no new crate.
- `docs/saves.md` with the full coverage table (every Bevy 0.20 world-serialization item, its TS
  form, status, test); the matrix row moves to covered.

Not in scope: web builds (bevy-ts ships none), `#[reflect(skip_serializing)]` for TS fields,
loading a save as an asset (`DynamicWorldRoot` from TSX), a `SettingsPlugin`.

## Capabilities

### New Capabilities

- `world-saves`: `saveWorld` / `loadWorld`, the `Save` marker, filters, the file format, entity
  remapping, TS migration and unknown classes on load, the result messages, the save folder, and
  the coverage table.

### Modified Capabilities

None. The ops are new entries in the command buffer; no existing requirement changes.

## Impact

- Code: new `crates/bevy-ts/src/saves.rs` (ops, `Save`, `TsSaved`, `TsSavedResources`, folder,
  messages); `crates/bevy-ts/src/commands.rs` (`OP_SAVE_WORLD`, `OP_LOAD_WORLD`);
  `crates/bevy-ts/src/lib.rs` (registration); `crates/bevy-ts-v8/js/core.js` and
  `packages/bevy-ts/index.d.ts` (the TS API; `packages/bevy-ts/runtime/core.js` regenerated);
  `crates/bevy-ts-cli/src/cli.rs` and `play.rs` (the folder per engine); tests in
  `crates/bevy-ts/tests/it/saves.rs`, `crates/bevy-ts-cli/tests/it/saves.rs`; a bench in
  `bench/world-saves`; `docs/saves.md`, `docs/research/bevy-coverage.md`.
- Dependencies: none new. `ron` and Bevy's `serialize` feature are already built
  (`bevy-ts-tsx/Cargo.toml:22`); `bevy-ts` may need `ron` as a direct dependency, already in the
  lock.
- Data: saves are written to the player's own user data folder, outside the game folder. That is
  a new place bevy-ts writes on a player's machine (ask-first: whose data, where).
- Performance: nothing per frame. A save and a load cost a world walk plus RON; the bench times
  both at 10k entities before landing.
