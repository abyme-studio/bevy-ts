## 1. Engine

- [ ] 1.1 `scene.rs`: `GltfChannels` on the root; the swap derives one instance per (root instance, source `StandardMaterial`), overwriting mapped params and textures; cache on the root, rebuilt when its material, filter or channels change.
- [ ] 1.2 `layout.rs`: `gltf` reserved, with `kind` and `gltfMaterial`.
- [ ] 1.3 `bevy-ts-tsx`: `resolve::script_material` takes `gltf`, checks names and channel kinds against the kind; `insert_script_material`/`remove_material` handle `GltfChannels`. `jsx-runtime.d.ts`: `gltf` typed by the kind's textures and `color` params.

## 2. Tests

- [ ] 2.1 A generated two-material glTF scene: each mesh's base colour texture reaches its instance; meshes of one material share an instance; a prop wins; a prop change re-derives; removal restores.
- [ ] 2.2 Errors: an unknown name, a wrong channel kind, a param named `gltf`.
- [ ] 2.3 Headless render: a toon kind on a two-texture scene draws both textures (two pixels, two colours).

## 3. Docs and gate

- [ ] 3.1 `docs/materials.md`: `gltf` on a scene root, with the Synty toon example and the `MaskTint` case.
- [ ] 3.2 The repo's gate, every cargo step through `gate-slot`.
