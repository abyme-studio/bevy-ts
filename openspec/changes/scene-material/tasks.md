## 1. The swap (`crates/bevy-ts-material`)

- [x] 1.1 `scene.rs`: `GltfMaterialFilter`, `SceneMaterialTargets`, `sync(world, root)` over the instance's entities, `scene_entities` and `SceneMaterialSet` public for scene-alpha, the `WorldInstanceReady` observer, change and removal systems; registered by `ScriptMaterialRenderPlugin`
- [x] 1.2 `layout.rs`: `Bindings::new` refuses a param or texture named `kind` or `gltfMaterial`

## 2. TSX (`crates/bevy-ts-tsx`, `packages/bevy-ts`)

- [x] 2.1 `resolve::script_material` takes `gltfMaterial` (a string) off the props; `Target::ScriptMaterial` carries it
- [x] 2.2 `insert_script_material` inserts or removes `GltfMaterialFilter`; `remove_material` removes it
- [x] 2.3 `jsx-runtime.d.ts`: `ScriptMaterialProps.gltfMaterial`

## 3. Tests and docs

- [x] 3.1 `bevy-ts-material/tests/scene.rs`, headless, a two-material glTF: every mesh swapped; only the named one; new props reach the meshes; a filter change; removal restores; a reloaded scene; a TSX child untouched
- [x] 3.2 `bevy-ts-cli/tests/scene_material.rs`: TSX end to end, V8 and `WorldAssetRoot`, a scene mesh carries the script material
- [x] 3.3 `docs/materials.md`: scene roots and `gltfMaterial`
- [x] 3.4 The full gate: build, workspace tests, clippy
