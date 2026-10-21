## 1. Writer

- [ ] 1.1 Optional `name` on `Op::Add` and a new `Op::Duplicate`; free-name picking from the base rules; tests for `Enemy`, `EnemyB2`, `EnemyB3`, a stem, and `exists` on a taken name
- [ ] 1.2 `duplicate`: copy the entity's text without leading comments, place it right after the source, inverse `delete`; refuse `prefab-internal` when not written
- [ ] 1.3 `reparent` with `index` (same parent reorders), renames `(old, new)` so `edit.rs` rekeys, inverse with the old index; `restore` honours `index`; tests that a reparent and its inverse, and a middle delete and its inverse, give back the file byte for byte
- [ ] 1.4 `add_component` (fields, internal `text`) and `remove_component` (whole patch), inverses, `exists` and `not-found` from the file
- [ ] 1.5 `add_entity` `asset`: glTF to `SceneRoot("…#Scene0")`, `.bsn` to an inclusion, otherwise `invalid`
- [ ] 1.6 Refuse a parent that is not written in the file with `prefab-internal`

## 2. Engine

- [ ] 2.1 `edit.rs`: world checks for `add_component` (`exists`) and `remove_component` (`prefab-internal` when the prefab gives it)
- [ ] 2.2 `created: [{ namePath, entity }]` in the result after the reconcile
- [ ] 2.3 Sidecar: `level_write` on a scene's sidecar path creates the file and opens the overlay; TSX entities of the scene refused for rename, delete, reparent, duplicate
- [ ] 2.4 Tests on the arena: reparent keeps entity bits and a TS component's state; an add returns the spawned entity; a sidecar add spawns in a scene with no level

## 3. Tree and node

- [ ] 3.1 `bevy-ts-tsx`: optional `level` on tree nodes and `sidecar` on scene roots through a hook; the levels crate fills them from `LevelInstance` and its parsed files
- [ ] 3.2 `level_node` `patches`
- [ ] 3.3 Tests: the arena's rows (`EnemyB` written, its eye and `RoomA/Lamp` not), a TSX scene root's `sidecar`

## 4. Catalog and shapes

- [ ] 4.1 `bevy_ts::Cube`, `Sphere`, `Plane` in the bevy-ts package, added to every manifest; the compile step reserves `bevy_ts::`
- [ ] 4.2 `bevy-ts/level_catalog`: project scenes with file, line, props; the six built-ins with their ops
- [ ] 4.3 Test: each built-in's op, sent as given at the top level of the arena, spawns its entity

## 5. Contract and docs

- [ ] 5.1 Protocol, the minor lead@bevy-ts allocates at landing: `level_catalog`, the new and changed `LevelOp` fields, `created`, `level` and `sidecar` on tree nodes, `patches`; regenerate Rust and TS types; CI diff check
- [ ] 5.2 `docs/levels.md` (new, linked from the README): editing a level through `level_write`, the ops, the catalog, sidecars; tell spec@bevy-outliner and spec@bevy-inspector the version it landed as
- [ ] 5.3 `cargo test` for `bevy-ts-levels` and `bevy-ts-tsx`, and the repo's full check, green
