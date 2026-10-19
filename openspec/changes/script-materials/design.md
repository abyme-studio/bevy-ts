## Context

Split out of the `native` change on 2026-10-20 (operator, applying the native review in docs/research/dx.md): script materials ship in the prebuilt engine and need no native crate, so they do not belong with project engines and plugin adoption. The research behind them is unchanged and still lives in `openspec/changes/native/design.md`'s context section (Bevy 0.20's erased material path, `manual_material.rs`, TypeGPU 0.12.8).

## Goals / Non-Goals

**Goals:**
- Custom materials without Rust: WGSL, a parameter layout and textures declared in TS at runtime, hot reloaded with the shaders.
- A verdict on TypeGPU.

**Non-Goals:**
- TS-authored render graph nodes, passes, compute or render-world code of any kind.

## Decisions

### 1. Script materials: one erased Rust material, runtime layouts
`bevy-ts-material` registers one Rust asset type, `ScriptMaterial`, implementing `ErasedRenderAsset` by hand, as `manual_material.rs` does: its `MaterialProperties` carry a bind group layout built per material kind from the TS schema, the kind's shader handles, and an `UnpreparedBindingResources` per instance (the uniform bytes and texture views). Rust lays params out with WGSL uniform rules and prepends the generated struct and bindings to the author's WGSL. Shader edits replace the shader asset in place; Bevy re-specializes.

- *Beat: one Rust `Material` type per material (`AsBindGroup` derive).* Every new material would be a Rust change and a project engine; the layout fn is static per type.
- *Beat: `ExtendedMaterial<StandardMaterial, _>` with a fixed extension of generic slots (N floats, M textures).* No layout logic, but authors index `params[3]` instead of naming fields, and the slot count is a cap someone will hit.
- *Beat: TS-authored render passes too.* Passes run in the render world on another thread with ordering rules (`chain_weak`) TS cannot see; settings components extracted by Rust cover the TS side.


### 2. TypeGPU: borrow ideas, do not adopt
Verdict: skip TypeGPU as a dependency; borrow its schema-per-WGSL-type idea for script-material params and its headless `resolve` as the model for generating WGSL declarations from a TS schema.

- **As the component layout vocabulary: no.** Its `d.*` types encode WGSL alignment (`vec3f` aligned 16, struct sizes rounded to 16), and components must follow C rules so they match Rust's `Vec3` (align 4) and `offset_of!` layouts of Rust components. Using WGSL layout for components would pad every `vec3` by a third and break the layout tier. It also has no notion of `entity`, `bool` as one byte, or enums.
- **As the material param vocabulary: borrowed, not imported.** A material's params are exactly a WGSL uniform struct, which is TypeGPU's model. bevy-ts's `bevy-ts/material` types mirror the `vec3f`-style names so authors who know TypeGPU or WGSL read them directly, and Rust computes the layout, as it does for components (bridge decision 3).
- **As the shader language: not now.** Its codegen works without a GPU device (`tgpu.resolve` is device-free), but JS-bodied functions need `unplugin-typegpu` at build time to embed their AST; bevy-ts compiles with oxc in the engine process and its loader rejects bare npm specifiers (bridge script-runtime), so adoption means porting the plugin to an oxc pass and vendoring a 0.x library that changes every few weeks into the built-in modules. Its resolved output is WGSL plus TypeGPU bind group layout objects that nothing maps to Bevy's `#{MATERIAL_BIND_GROUP}` conventions or PBR imports. WGSL strings with generated bindings give custom materials today with none of that.
- **Owning the GPU: never on native.** Bevy's wgpu device is the only one. On the web, both would sit on the browser's WebGPU, which is still not a reason to give TypeGPU the device.
- Revisit when TypeGPU reaches 1.0 and offers a codegen-only entry that emits WGSL functions for a host's bind group conventions; a `typegpu`-authored WGSL string can already be pasted into `fragment` today.


## Risks / Trade-offs

- [`ScriptMaterial` puts many layouts behind one `TypeId`-keyed bind group allocator] → `allocate_unprepared` takes the layout per call, so non-bindless allocation per kind should work; task 1.1 is a spike, and the fallback is one Rust type per kind from a fixed pool registered at startup.

## Open Questions

- Whether material instances are assets addressed by key or components on the entity (task 1.4).
