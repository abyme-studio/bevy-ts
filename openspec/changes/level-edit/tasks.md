## 1. Writer

- [x] 1.1 Optional `name` on `Op::Add`; free-name picking from the base rules; tests for `Enemy`, `Enemy2`, an asset stem, `Entity`, and `exists` on a taken name
- [x] 1.2 `reparent` with `index` (same parent reorders), renames `(old, new)` so `edit.rs` rekeys, inverse with the old index; `restore` honours `index`; tests that a reparent and its inverse, and a middle delete and its inverse, give back the file byte for byte
- [x] 1.3 `add_component` (fields, internal `text`) and `remove_component` (whole patch), inverses, `exists` and `not-found` from the file
- [x] 1.4 `add_entity` `asset`: glTF to `SceneRoot("…#Scene0")`, `.bsn` to an inclusion, otherwise `invalid`
- [x] 1.5 Refuse a parent that is not written in the file with `prefab-internal`

## 2. Engine

- [x] 2.1 `edit.rs`: world checks for `add_component` (`exists`) and `remove_component` (`prefab-internal` when the prefab gives it)
- [x] 2.2 `created: [{ namePath, entity }]` in the result after the reconcile
- [x] 2.3 Sidecar: `level_write` on a scene's sidecar path creates the file and opens the overlay; TSX entities of the scene refused for rename, delete, reparent
- [x] 2.4 Tests on the arena: reparent keeps entity bits and a TS component's state; an add returns the spawned entity; a sidecar add spawns in a scene with no level

## 3. Tree and node

- [x] 3.1 `bevy-ts-tsx`: optional `level` on tree nodes and `sidecar` on scene roots through a hook; the levels crate fills them from `LevelInstance` and its parsed files
- [x] 3.2 `level_node` `patches`
- [x] 3.3 Tests: the arena's rows (`EnemyB` written, its eye and `RoomA/Lamp` not), a TSX scene root's `sidecar`

## 4. Catalog

- [x] 4.1 `bevy-ts/level_catalog`: the generation's placeable scene components with file, line, props
- [x] 4.2 Test: the arena's catalog lists `enemy::Enemy` and nothing else

## 5. Contract and docs

- [x] 5.1 Protocol, the minor lead@bevy-ts allocates at landing: `level_catalog`, the new and changed `LevelOp` fields, `created`, `level` and `sidecar` on tree nodes, `patches`; regenerate Rust and TS types; CI diff check
- [ ] 5.2 `docs/levels.md` (new, linked from the README): editing a level through `level_write`, the ops, the catalog, sidecars, and why there are no built-in primitives or duplicate (Bevy's editor has neither); tell spec@bevy-outliner and spec@bevy-inspector the version it landed as
- [x] 5.3 `cargo test` for `bevy-ts-levels` and `bevy-ts-tsx`, and the repo's full check, green
