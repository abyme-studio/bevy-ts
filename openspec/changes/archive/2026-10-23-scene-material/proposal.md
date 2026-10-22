## Why

A `<material kind>` on an entity that spawns a glTF scene (`WorldAssetRoot`) draws nothing today: the script material lands on the root, which has no mesh, and the scene's meshes keep their own `StandardMaterial`s. agent-town tints Synty ship and station hulls by repo colour through a mask (`game::MaskTint`: textures `base` and `mask`, `colors: array(color, 3)`), so it needs a script material on every hull mesh of a spawned scene, and only on the hull panels.

## What Changes

- **A script material on a scene root draws the scene's meshes.** Once the scene under a `WorldAssetRoot` is ready, each of its mesh entities with a `StandardMaterial` swaps it for the root's `ScriptMaterial3d`. The original handle is kept.
- **`gltfMaterial="Name"`** on `<material kind>` limits that to meshes whose glTF material is named `Name` (Bevy's `GltfMaterialName`, exact match). Without it, every mesh of the scene.
- **Changing the props updates them**: the new instance reaches every swapped mesh in the same frame. Changing `gltfMaterial` restores the meshes that stop matching and swaps the ones that start.
- **Removing the material restores the scene's own materials.** A reloaded scene (ready again) is swapped again.
- **`kind` and `gltfMaterial` are reserved**: `material()` refuses a param or texture by either name, since `<material>` takes them itself. A param named `kind` could never be set; this makes that an error.
- Rust callers get the same with `ScriptMaterial3d` plus an optional `GltfMaterialFilter` on the root.
- Owned by `bevy-ts-material` (lead@script-gltf, 2026-10-23). It hooks `WorldInstanceReady` as script-gltf's scene-animation does, and walks the instance's own entities, not the root's descendants, so TSX children are never touched.
- **Out of scope:** `<material color>` (a `StandardMaterial`) on a scene root; per-mesh different materials in one scene (one root, one kind); prefix or pattern matches.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `script-materials`: a script material on a glTF scene root draws the scene's meshes, optionally by glTF material name; `kind` and `gltfMaterial` are reserved prop names.

## Impact

- `crates/bevy-ts-material`: new `scene.rs` (the swap, its observer and systems, `GltfMaterialFilter`); `layout.rs` refuses the reserved names.
- `crates/bevy-ts-tsx`: `resolve::script_material` takes `gltfMaterial` off the props; `insert_script_material` inserts or removes `GltfMaterialFilter`; `remove_material` removes it.
- `packages/bevy-ts/jsx-runtime.d.ts`: `ScriptMaterialProps.gltfMaterial`.
- `docs/materials.md`. No protocol change.
