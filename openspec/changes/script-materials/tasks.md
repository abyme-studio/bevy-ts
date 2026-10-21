## 1. Script materials

- [x] 1.1 Spike: one `ScriptMaterial` asset type through `ErasedRenderAsset` with two kinds of different bind group layouts behind its single `TypeId` allocator, non-bindless; fallback to a fixed pool of Rust types if the allocator refuses
- [x] 1.2 WGSL uniform layout in Rust for material params; generate the params struct and bindings and append them after the author's WESL; unit-test sizes and offsets (`vec3f` at 16, arrays strided to 16)
- [x] 1.3 Loader: resolve a relative `.wgsl` import to a shader-file value, watch the file, fail a missing file with the importing line; ship `declare module "*.wgsl"` in the bundled types
- [x] 1.4 `bevy-ts/material`'s `material()` declaration in the generation manifest, `fragment`/`vertex` as shader-file values (strings rejected); a `.wgsl` save replaces the shader asset in place with no new generation; param-schema changes re-encode instances by name
- [x] 1.5 WGSL errors: check with `wesl` and `naga`, map spans to the author's `file:line:col` (author lines unchanged, declarations appended), report an error past the author's last line against the TS schema, keep the old shader; tests for both
- [x] 1.6 Decide whether instances are keyed assets or components on the entity, then add TSX `<material kind={...} />`
- [ ] 1.7 Demo: a `game::Glow` material on the cube example with its shader in `glow.wgsl`, edited live, in the prebuilt engine with no native crate
