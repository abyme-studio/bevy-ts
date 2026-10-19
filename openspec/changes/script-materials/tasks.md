## 1. Script materials

- [ ] 1.1 Spike: one `ScriptMaterial` asset type through `ErasedRenderAsset` with two kinds of different bind group layouts behind its single `TypeId` allocator, non-bindless; fallback to a fixed pool of Rust types if the allocator refuses
- [ ] 1.2 WGSL uniform layout in Rust for material params; generate the params struct and bindings and prepend them to the author's WGSL; unit-test sizes and offsets (`vec3f` at 16, arrays strided to 16)
- [ ] 1.3 `bevy-ts/material`'s `material()` declaration in the generation manifest; shader replacement in place on source change; param-schema changes re-encode instances by name; WGSL errors mapped to the material and source line, old shader kept
- [ ] 1.4 Decide whether instances are keyed assets or components on the entity, then add TSX `<material kind={...} />`
- [ ] 1.5 Demo: a `game::Glow` material on the cube example, edited live, in the prebuilt engine with no native crate
