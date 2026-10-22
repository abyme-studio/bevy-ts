## Context

`<material kind={K}>` inserts `ScriptMaterial3d(h)` on its entity (`bevy-ts-tsx/src/intrinsics.rs`). On a `WorldAssetRoot` entity that is the scene root; the meshes are the scene's own entities, spawned later by `WorldInstanceSpawner`, each with `Mesh3d`, `MeshMaterial3d<StandardMaterial>` and, when the glTF names it, `GltfMaterialName` (`bevy_gltf` loader, on the mesh entity). script-gltf's scene-animation reacts to `WorldInstanceReady` the same way.

## Decisions

1. **The root's `ScriptMaterial3d` is the source of truth.** No new TSX target: the root keeps it (it draws nothing there, having no mesh) and `bevy-ts-material` copies it down. A Rust caller writes the same two components. Hot reload re-encodes instances in place, so meshes sharing the handle follow with no extra work.
2. **The instance's entities, not the descendants.** `WorldInstanceSpawner::iter_instance_entities(instance)` lists exactly what the scene spawned. TSX children of the root, which may have their own meshes and materials, are never swapped. No shared descendants helper is needed.
3. **Originals are kept on the root**, in `SceneMaterialTargets` (mesh entity, original `StandardMaterial` handle). One sync function brings a root to what it asks: restore targets that no longer match (or all, with no material), swap matching meshes not yet swapped, refresh the handle on the rest. It runs on `WorldInstanceReady` (stale targets of a respawned scene are dropped), on a changed `ScriptMaterial3d` or `GltfMaterialFilter` once the instance is ready, and on removal of either.
4. **`gltfMaterial` is a reserved prop**, exact match on `GltfMaterialName`. Synty names whole material slots (hull panels), so one exact name covers the need; patterns can come later. `material()` refuses params or textures named `kind` or `gltfMaterial`.
5. **Swap means remove `MeshMaterial3d<StandardMaterial>`, insert `ScriptMaterial3d`**, as `insert_script_material` does on a plain mesh, so one draws.

## Risks

- A scene mesh with no `StandardMaterial` (another material type) is left alone.
- A mesh both swapped and later given its own material by a system is restored over on removal. Accepted: the scene's meshes belong to the scene.
