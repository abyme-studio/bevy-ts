## 1. Protocol

- [ ] 1.1 `protocol/bevy-ts.openrpc.json` at the next free minor on main when this lands (planned 1.13.0, after engine-components 1.11.0 and viewport-tools 1.12.0; lead@bevy-ts confirms the number), and the same number in the shared-surface spec: `AssetInfo.thumbnail` (boolean), `PickHit.point` and `ViewportPickResult.ground` (nullable) as `[number, number, number]`
- [ ] 1.2 Regenerate Rust and TS types through bridge's pipeline (`packages/bevy-ts/protocol.d.ts`, `crates/bevy-ts-remote`); the contract tests pass

## 2. The listing

- [ ] 2.1 `has_thumbnail(path)` beside `kind()` in `crates/bevy-ts-viewport/src/assets.rs`: `image`, `mesh` and `.bsn`; `asset_thumbnail` answers `png: null` at once whenever it is false
- [ ] 2.2 `thumbnail` on every `AssetInfo` from `assets` and `assets+watch`; unit test over every kind and `.scn.ron`

## 3. Level thumbnails

- [ ] 3.1 In `crates/bevy-ts-levels`, a function that composes a level (`compose_level`) and spawns its resolved nodes as a detached tree keeping only `Transform`, `Visibility`, mesh and material intrinsics, `Mesh3d`/`MeshMaterial3d`, lights and glTF scene roots, each entity spawned with `EditorOnly`, `ScriptHidden` and layer 30 in its first insert; it returns the tree's root or the compose errors
- [ ] 3.2 A level job in `assets.rs`: load the `.bsn` and its includes, wait for `is_loaded_with_dependencies`, spawn through 4.1, then the existing render, frame and read-back steps; a compose error caches `png: null`
- [ ] 3.3 Test on `examples/arena` while it plays: `levels/arena.bsn` at 128 answers `pending`, then a 128×128 PNG with drawn pixels; meanwhile no TS system saw a new `Foe`, `tsx_tree` and `entities+watch` are unchanged, no rapier body was added, and every thumbnail entity is gone after the read-back
- [ ] 3.4 Measure the arena level and a generated 200-entity level: compose time, frames to the PNG, and frame time while it renders. Write the numbers in `docs/viewport.md` and in the report to lead@bevy-ts

## 4. Pick points

- [ ] 4.1 `point` per hit (`origin + direction × depth`) and `ground` (the ray on y = 0 in front of the camera, else null) in `edit.rs` and `viewport_pick`
- [ ] 4.2 Tests: a pick on a cube gives a point on its surface; a pick below the horizon on empty floor gives `ground` with y = 0; a pick at the sky gives `ground: null`

## 5. The arena's assets

- [ ] 5.1 A test helper generates `examples/arena/assets/models/crate.gltf` (embedded buffer, one PBR material), `textures/checker.png` (64×64, via `assets.rs`'s PNG writer) and `sounds/hit.wav` (a short sine); commit the files
- [ ] 5.2 `bevy-ts/assets` on the arena lists `mesh`, `image`, `audio` and `scene`; the crate's thumbnail renders; the levels walk and every arena test still pass

## 6. Docs and specs

- [ ] 6.1 `docs/viewport.md`: the asset methods (kinds, `thumbnail`, what each kind draws, `pending`) and `viewport_pick`'s `point` and `ground`
- [ ] 6.2 `cargo test` for `bevy-ts-viewport`, `bevy-ts-levels`, `bevy-ts-cli`, and `openspec validate asset-catalog --strict` pass
