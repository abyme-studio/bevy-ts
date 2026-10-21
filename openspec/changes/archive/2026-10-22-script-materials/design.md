## Context

Split out of the `native` change on 2026-10-20 (operator, applying the native review in docs/research/dx.md): script materials ship in the prebuilt engine and need no native crate, so they do not belong with project engines and plugin adoption. The research behind them is unchanged and still lives in `openspec/changes/native/design.md`'s context section (Bevy 0.20's erased material path, `manual_material.rs`, TypeGPU 0.12.8).

## Goals / Non-Goals

**Goals:**
- Custom materials without Rust: a parameter layout and textures declared in TS at runtime, shaders in `.wgsl` files beside the TS, hot reloaded in place.
- WGSL errors that point at the author's file and line.
- A verdict on TypeGPU.

**Non-Goals:**
- TS-authored render graph nodes, passes, compute or render-world code of any kind.

## Decisions

### 1. Script materials: one erased Rust material, runtime layouts
`bevy-ts-material` registers one Rust asset type, `ScriptMaterial`, implementing `ErasedRenderAsset` by hand, as `manual_material.rs` does: its `MaterialProperties` carry a bind group layout built per material kind from the TS schema, the kind's shader handles, and an `UnpreparedBindingResources` per instance (the uniform bytes and texture views). Rust lays params out with WGSL uniform rules and appends the generated struct and `@group(3)` bindings after the author's last line. Bevy 0.20 compiles material shaders as WESL, whose imports (`import bevy_pbr::render::forward_io::VertexOutput;`) must come first, and appending keeps the author's line numbers as they are. Shader edits replace the shader asset in place; Bevy re-specializes.

- *Beat: one Rust `Material` type per material (`AsBindGroup` derive).* Every new material would be a Rust change and a project engine; the layout fn is static per type.
- *Beat: `ExtendedMaterial<StandardMaterial, _>` with a fixed extension of generic slots (N floats, M textures).* No layout logic, but authors index `params[3]` instead of naming fields, and the slot count is a cap someone will hit.
- *Beat: TS-authored render passes too.* Passes run in the render world on another thread with ordering rules (`chain_weak`) TS cannot see; settings components extracted by Rust cover the TS side.


### 2. TypeGPU: borrow ideas, do not adopt
Verdict: skip TypeGPU as a dependency; borrow its schema-per-WGSL-type idea for script-material params and its headless `resolve` as the model for generating WGSL declarations from a TS schema.

- **As the component layout vocabulary: no.** Its `d.*` types encode WGSL alignment (`vec3f` aligned 16, struct sizes rounded to 16), and components must follow C rules so they match Rust's `Vec3` (align 4) and `offset_of!` layouts of Rust components. Using WGSL layout for components would pad every `vec3` by a third and break the layout tier. It also has no notion of `entity`, `bool` as one byte, or enums.
- **As the material param vocabulary: borrowed, not imported.** A material's params are exactly a WGSL uniform struct, which is TypeGPU's model. bevy-ts's `bevy-ts/material` types mirror the `vec3f`-style names so authors who know TypeGPU or WGSL read them directly, and Rust computes the layout, as it does for components (bridge decision 3).
- **As the shader language: not now.** Its codegen works without a GPU device (`tgpu.resolve` is device-free), but JS-bodied functions need `unplugin-typegpu` at build time to embed their AST; bevy-ts compiles with oxc in the engine process and its loader rejects bare npm specifiers (bridge script-runtime), so adoption means porting the plugin to an oxc pass and vendoring a 0.x library that changes every few weeks into the built-in modules. Its resolved output is WGSL plus TypeGPU bind group layout objects that nothing maps to Bevy's material bind group (`@group(3)`) conventions or PBR imports. WGSL files with generated bindings give custom materials today with none of that.
- **Owning the GPU: never on native.** Bevy's wgpu device is the only one. On the web, both would sit on the browser's WebGPU, which is still not a reason to give TypeGPU the device.
- Revisit when TypeGPU reaches 1.0 and offers a codegen-only entry that emits WGSL functions for a host's bind group conventions; WGSL that `typegpu` resolves can already be saved into a `.wgsl` file today.

### 3. Shaders are `.wgsl` files, imported like modules
Operator ruling 2026-10-21: a material's shaders are `.wgsl` files next to the game's TS, not WGSL strings in TS. `material()` takes them as imports: `import glow from "./glow.wgsl"`, then `fragment: glow`. The loader resolves the import to a synthetic module whose default export is an opaque shader-file value (its resolved path); the WGSL text stays host-side and never enters V8.

- The loader already resolves relative imports against the importing module and watches every loaded file (hot-reload). A `.wgsl` import joins that graph, so watching costs nothing new, and a typo fails the load with the importing file and line, as `lodash` does.
- The path is relative to the TS file that names it, the same rule as every other import. A string path would need its own rule (relative to the caller? to the entry? to `assets/`?), and `material()` cannot see its caller's file without a stack walk.
- Editors and `tsc` follow it: one `declare module "*.wgsl"` in the bundled types, and go-to-definition opens the file.
- A `.wgsl` save does not start a generation: the host sees a non-JS file changed, reads it, and replaces the shader asset of every material naming it. No JS reruns and no state is touched.
- Errors: the declarations are appended, so a span on combined line `n` at or below the author's line count `k` is `file:n` unchanged; a span at `n > k` is in generated code, so it reports against the material's params in TS (the usual cause is a schema the shader no longer matches).

- *Beat: a relative path string, `fragment: "./glow.wgsl"`.* Same file on disk, but resolution needs a second rule and a caller-file lookup, the file is not in the module graph so it needs its own watch list, and a typo is found late, when the material is built.
- *Beat: a Bevy asset path under `assets/`, loaded by `AssetServer`.* Moves shaders away from the TS that owns their schema, depends on Bevy's `file_watcher` feature for reload, and its errors name the combined source Bevy compiled, not the author's line.
- *Beat: WGSL strings in TS (the earlier draft).* Overruled; also no WGSL highlighting or tooling, and errors needed a template-literal offset.

## Risks / Trade-offs

- [A `.wgsl` file that imports of another project `.wgsl` file] → out of scope: only Bevy's own shader imports resolve; a later change can add project imports through the same loader.

- [`ScriptMaterial` puts many layouts behind one `TypeId`-keyed bind group allocator] → `allocate_unprepared` takes the layout per call, so non-bindless allocation per kind should work; task 1.1 is a spike, and the fallback is one Rust type per kind from a fixed pool registered at startup.

### 4. Instances are shared assets keyed by kind and props
Resolved in task 1.6 (build, 2026-10-21). `<material kind={...} />` resolves to a `ScriptMaterial` asset keyed by (kind, props) and shared by equal props, the same cache `<material>` uses for `StandardMaterial`. The entity holds `ScriptMaterial3d(handle)`.

- Equal props share one bind group, so a hundred glowing cubes cost one.
- Bound params (`prop={() => ...}`) are not supported yet, as on the other asset intrinsics; they would need per-entity instances or a per-frame upload, a later change.
- *Beat: a component on the entity holding the params.* Every entity would carry its own uniform buffer and bind group, and it would differ from how `<material>` already works.

## Open Questions

None.
