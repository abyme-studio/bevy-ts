## Why

A level that names a component whose code is gone, renamed, broken or not built yet fails to load whole. One deleted class makes `arena.bsn` unopenable, and a TS save that deletes a class a level uses is refused. The studio is being rebuilt on Unity's frame (the bevy studio first-principles study, change E3). Unity loads such a scene with a "missing script" in place and keeps its data. bevy-ts should do the same: load the entity, mark what is missing, and keep the saved text.

## What Changes

- **A missing component becomes a placeholder, not a load error.** In bevy-ts's own `.bsn` loader, three cases no longer fail the level:
  - `unregistered`: no live code registers the type (a scene component or a plain patch).
  - `failed`: its code threw (`scene()`), or the TS build failed and left the empty generation.
  - `mismatch`: the code is live but some saved fields or props do not fit (renamed, retyped, a required prop unset).
- **The entity still spawns** under its name path with everything that did resolve. It gains one reflected marker, `MissingComponents`, listing each entry: component, reason, message, saved fields as written, location, and `expected` when known. Queries never match the missing type, so nothing runs for it.
- **A mismatch applies what fits.** The component is live with the fields and props that fit. Only the misfits are listed, each with why.
- **A missing scene instance keeps its inner overrides.** Child entries with no base of their own are kept on the marker, not spawned. Children with their own base spawn as usual.
- **The file is the store.** The writer already splices text, so save, rename, reparent, delete and undo, duplicate and paste keep a placeholder's text byte for byte. Switching play and edit keeps the marker.
- **When the code comes back**, the next reconcile adds the component to the same entity and drops the entry. No respawn, no extra event.
- **BREAKING** `level-hot-reload` "A bad file changes nothing" narrows to broken files: a parse error, a duplicate name, an inclusion cycle and a bound-field override still reject the file whole. Missing code no longer does.
- **BREAKING** `level-hot-reload` "TS reloads re-expand levels inside the generation" (rule 9, `check_section`): a generation that drops a class a level uses commits. Its instances become placeholders, with a warning naming the level lines. Broken-file errors under the new generation still fail it.
- **Strict where agents act and where the game ships.**
  - `level_write` refuses `set`, `set_prop` and `revert` on a missing component (`missing`), and any op that writes a type no live code registers (`unknown`). `remove_component` and every whole-entity op still work.
  - `bevy-ts levels check` is unchanged: it never resolved types, and it still fails on parse errors.
  - A shipped game (`dev(false)`) keeps the strict loader: a missing component fails the level, as today. `bevy-ts build` is unchanged.
- **Protocol: the next free minor on main at landing** (1.21: touch-time took 1.20), additions only:
  - `TsxTreeNode.level.missing?: { component, reason }[]`, so the Hierarchy marks a row with no call per row.
  - `LevelNodeResult.missing?: MissingComponent[]` with `component`, `reason`, `message?`, `fields`, `location`, `expected?`, `misfit?` and `overrides?`, so the Inspector draws it.
  - `level_write` refusal reasons gain `missing` and `unknown`.
  - Each placeholder entry is a warning in the level's diagnostics, on `level_list`, `level_open` and `level+watch`'s `loaded`.
- **No last-good schema cache in v1.** `schema` is absent from `MissingComponent`. `expected` comes from the declaration the process last saw live, and is absent after a restart.
- **Out of scope:** the studio's Hierarchy mark and Inspector group (abyme, design@studio-placeholders and build@studio-placeholders, after this release); Play/Stop snapshots (E4), which must keep `MissingComponents` when they land; a cached schema; making `levels check` or `build` resolve types.

### The Bevy-principles exception

bevy-ts adds no editor or engine feature Bevy lacks or is not discussing. This change is one. Bevy fails a whole scene on an unknown type (`DynamicWorld`'s deserializer, `WorldInstanceSpawnError::UnregisteredType`), and no upstream issue or PR proposes tolerating one. The exception is bounded:
- It lives only in bevy-ts's own `.bsn` loader, which bevy-ts owns until Bevy ships one (`bsn-format`). It changes nothing in reflection, in Bevy's scene types or in a shipped game.
- When bevy-ts adopts Bevy's loader (`bsn-format`'s "a deletion"), placeholders go with it or are argued upstream first.
- It needs the operator's ruling (`level-placeholders/go`).

## Capabilities

### New Capabilities

- `level-placeholders`: what a level does with a component whose code is missing, failed or does not fit: the placeholder entity, its marker, kept overrides, what survives, how it reports over the protocol, and where tolerance stops.

### Modified Capabilities

- `levels`: an unknown scene component, prop or field no longer fails the load; opening a level with missing code opens it.
- `level-hot-reload`: a bad file is a broken file, not missing code; a TS generation that drops a used class commits.
- `level-writing`: `level_write` refuses writes to a missing component and writes of an unknown type.

## Impact

- `crates/bevy-ts-levels/src`: `compose.rs` (failures become placeholder entries; kept overrides), `engine.rs` (`check_section` keeps only broken-file errors; `MissingComponents` on reconcile; diagnostics), `edit.rs` (`missing` and `unknown` refusals), `protocol.rs` (`level.missing`, `LevelNodeResult.missing`). Tests in `crates/bevy-ts-levels/tests/levels.rs`.
- `crates/bevy-ts-tsx/src/host.rs`: `expand` returns each item's result, so a throwing `scene()` fails its item only. The JS side already returns `null` per item.
- `bevy_ts` plugin: the strict loader under `dev(false)`.
- `protocol/bevy-ts.openrpc.json`: the next free minor at landing, regenerated Rust and TS types. lead@bevy-ts names the number.
- `docs/levels.md`: placeholders, what stays strict, and the Bevy-principles exception.
- abyme: the studio reads `level.missing` and `LevelNodeResult.missing` (build@studio-placeholders). An older engine sends neither, and fails the load as before.
- Performance: none at rest. Compose already walks every entry; a placeholder adds one small component.
- Needs the operator's go: the Bevy-principles exception (open in `level-placeholders/go`). Nothing else on the ask-first list: no new dependency, no new infrastructure (no cache, nothing new on disk), no performance cost, nothing irreversible. Data risk falls: the text is never rewritten, and a TS save that deletes a used class stops being refused.
