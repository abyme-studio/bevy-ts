## Context

Script materials (`docs/materials.md`, change `2026-10-22-script-materials`) draw a `<mesh>` with a WGSL file and a params schema declared in TS. One Rust asset, `ScriptMaterial`, implements `ErasedRenderAsset` by hand and returns Bevy's `PreparedMaterial` with its kind's layout and shaders (`render.rs`). Script passes (`pass.rs`) reuse its `layout.rs` and `wgsl.rs`.

Bevy 0.20's 2D materials are erased the same way (`bevy_sprite_render-0.20.0/src/mesh2d/material.rs`): `MeshMaterial2d<M>` is an `ErasedRenderAsset` whose `ErasedAsset` is `PreparedMaterial2d { binding, properties: Arc<MaterialProperties> }` (line 1209). The shaders, layout, alpha mode, material key and draw functions are in `properties`, per prepared asset. `Material2dPipelineSpecializer::specialize` (line 505) reads the shaders and layout from those properties and inserts the layout at group 2. `SetMaterial2dBindGroup` (line 570) finds the allocator by the asset's `TypeId`. Only the trait `Material2d` ties a type to static shaders; the pipeline under it does not. 2D meshes take `MeshTag` (`mesh.rs:291`, `functions.wesl:46 get_tag`).

The use is learn's star map (agent-town, research doc `learn-planets-research.md`): up to 500 planets of up to four stacked quads, crisp under `PixelCamera` zoom 3.

## Goals / Non-Goals

**Goals:**
- A script declares a 2D material with params, instance params, textures and a `.wgsl` fragment, and draws a `<mesh2d>` with it, with no Rust.
- Hot reload and errors as 3D script materials have them.
- Pixel art stays crisp, and the shader has a stated way to find its art pixel.
- A measured frame cost, with a gate.

**Non-Goals:**
- A script material on a `Sprite` (decision 7).
- `Kind.add` owned instances and `assets.set` in 2D.
- A script material on a glTF scene, `MaterialAlpha`, or a `TilemapChunk`.
- Prepass, shadows or a depth bias: 2D has none.
- Any JS in the render world.

## Decisions

### 1. One erased 2D asset, beside the 3D one
`material2d.rs` defines an asset `ScriptMaterial2d { kind: Arc<Kind>, data, textures }`, the fields `ScriptMaterial` has, and a component `MeshScriptMaterial2d(Handle<ScriptMaterial2d>)`, as `ScriptMaterial3d` holds a `Handle<ScriptMaterial>`. `ScriptMaterial2d` implements `ErasedRenderAsset` with `ErasedAsset = PreparedMaterial2d`, as `render.rs` does for 3D: it builds the bind group through the one non-bindless allocator under `TypeId::of::<ScriptMaterial2d>()`, then `MaterialProperties` with the kind's layout, `Material2dFragmentShader` and `Material2dVertexShader`, `ErasedMaterialKey::new(kind.id)`, `mesh_pipeline_key_bits` from the alpha mode (`alpha_mode_pipeline_key_2d`), Bevy's 2D `base_specialize`, and the three 2D draw functions.

- *Not* `MeshMaterial2d<ScriptMaterial2d>`: `MeshMaterial2d<M>` needs `M: Material2d`, and Bevy implements `ErasedRenderAsset` for every `MeshMaterial2d<M>` with `M`'s static shaders. Our own component is the only way to carry a kind's shader at runtime. The brief's name moves to `MeshScriptMaterial2d`.
- *Not* the 3D asset with a second render impl: two `ErasedRenderAssetPlugin`s on `ScriptMaterial` would prepare every 3D instance as 2D too, and both would share one allocator.
- Bevy's `DrawMaterial2d` is `pub(super)`. `material2d.rs` declares the same tuple (`SetItemPipeline`, `SetMesh2dViewBindGroup<0>`, `SetMesh2dBindGroup<1>`, `SetMaterial2dBindGroup<2>`, `DrawMesh2d`) as `DrawScriptMaterial2d` and adds it to the three 2D phases with `add_render_command`.
- Bevy's per-`M` systems (`extract_mesh_materials_2d`, `extract_entities_needs_specialization`, `check_entities_needing_specialization`, `mark_2d_meshes_as_changed_if_their_materials_changed`) are generic over `M: Material2d`. `material2d.rs` writes their bodies for `MeshScriptMaterial2d`, writing into Bevy's shared `RenderMaterial2dInstances` and 2D specialisation caches, as `render.rs` does for 3D. `queue_material2d_meshes` and `specialize_material2d_meshes` are not generic: they read those shared resources and draw our entities.

Task 1.1 is a spike of exactly this. If `specialize_material2d_meshes` does not see an entity no `Material2dPlugin` registered, or the allocator lookup fails, it stops and reports before the rest.

### 2. Shared with 3D, and what is new
Shared, unchanged: param and instance layout (`layout.rs`), name rules, colour and array encoding, the WGSL check and its error mapping (`wgsl.rs`), the kinds manifest and its check and apply (`kinds.rs`), the instance buffer and `MeshTag` rows (`instance.rs`), `PipelineWatch`, the image-sampler rebinding, keyed instances and their sweep.

New or parameterised:
- `Bindings` takes a space, 2D or 3D, which sets the group (`@group(2)` in 2D, `@group(3)` in 3D) and the `get_tag` import (`bevy_sprite_render::mesh2d::functions::get_tag` in 2D, `bevy_pbr`'s in 3D), both still imported on the author's first line as `bevy_ts_get_tag`.
- `wgsl.rs` checks a 2D kind against Bevy's 2D modules with the shader defs `Mesh2dPipeline` specialises with (`VERTEX_UVS`, `VERTEX_POSITIONS`, the tonemapping defs), taken from the live pipeline as 3D's are.
- `kinds.rs`: a `materials` entry carries `space: "2d" | "3d"`. One namespace of kind names across both: a 2D and a 3D kind of one name fail the load at the second call. A kind whose space changes on a reload is a new kind.
- Alpha: `AlphaMode2d`. `material2d()` takes `"opaque"`, `"blend"` and `{ mask }`; `"premultiplied"`, `"add"`, `"multiply"` and `"alphaToCoverage"` fail at the call naming the three.
- `instance.rs` gives a row to an entity holding either `ScriptMaterial3d` or `MeshScriptMaterial2d`. The rows stay one buffer for both spaces.

### 3. `<material2d kind>` in TSX
`<material2d kind={Planet} seed={3} land={() => cutoff()} />` sits beside a `<mesh2d>`, as `<material kind>` beside a `<mesh>`. Props split as on `<material kind>` (`kinds::split_props`): params and textures make the key of a shared `ScriptMaterial2d`, equal props share one asset, and an instance no entity holds is freed at the frame's end; instance props go to the entity's `ScriptInstance`, bindable. Without `kind` it is today's `ColorMaterial`, and `kind` with a `ColorMaterial` prop (`color`, `alpha`, `texture`, `uvTransform`) that is not one of the kind's params fails at the element. A 3D kind on `<material2d>`, or a 2D kind on `<material>`, fails at the element naming the right element. `<material2d kind>` with `<mesh>` fails as `<material2d>` does today.

### 4. Generated declarations
As 3D, after the author's last line:

```wgsl
struct PlanetParams { light: vec2<f32>, colors: array<vec4<f32>, 4> }
@group(2) @binding(0) var<uniform> params: PlanetParams;
@group(2) @binding(1) var noise: texture_2d<f32>;
@group(2) @binding(2) var noise_sampler: sampler;
struct PlanetInstance { pixels: f32, seed: f32, land: f32, rotation: f32 }
@group(2) @binding(3) var<storage, read> instances: array<PlanetInstance>;
fn instance(i: u32) -> PlanetInstance { return instances[bevy_ts_get_tag(i)]; }
```

The author's file imports `bevy_sprite_render::mesh2d::vertex_output::VertexOutput` (its `uv`, `world_position`, `instance_index`) and globals from `bevy_sprite_render::mesh2d::view_bindings::globals` for `globals.time`, which pauses with virtual time. The spike confirms the WESL paths; the doc names what it found.

### 5. Crisp pixel art
`PixelCamera` at a whole zoom maps one world unit to a whole `zoom × zoom` block of screen pixels, and its snap puts the blocks on whole screen pixels (`docs/2d.md`, Pixel camera). The fragment runs once per screen pixel. So the engine guarantees: every value the shader computes from `floor(in.world_position.xy)` is the same across a block. A shader gets its art pixel one of two ways, both in the doc and both tested:

- Quad space: `floor(in.uv * pixels) / pixels`, `pixels` the quad's size in world units (an instance param), as PixelPlanets does. Crisp when the quad's corners sit on whole world units: a whole translation for an even size, a half one for an odd size. bevy-ts does not move the mesh; the doc says to place it.
- World space: `floor(in.world_position.xy)`. Crisp wherever the mesh is, with the art fixed to the world grid.

Textures sample with the image's own sampler, nearest under `"imageSampler": "nearest"`, as 3D textures do. `PixelCamera` turns off MSAA, so a quad's edge covers whole pixels. `<YSort>` sets a script-material mesh's z and marks its `Mesh2d` changed as it does for any 2D mesh (`Pixel2dSystems`), so it is queued again at its new z; a test proves the lower of two y-sorted quads draws in front. Stacked layers of one planet are children at local z steps of 0.00001, below one y pixel, as a sprite's shadow is.

### 6. Hot reload and errors, as 3D
A `.wgsl` save checks the file with the kind's declarations and replaces its shader in place: no JS, no generation. A failing check keeps the last good shader; with none, the mesh draws nothing until fixed. A changed schema re-encodes every keyed asset and instance row by name, and waits on a shader that no longer checks. Renderer failures go through `PipelineWatch` with versions, the same watch 3D uses.

### 7. Sprites, a follow-up
Bevy 0.20 draws every `Sprite` as a unit-quad `Mesh2d` with a `SpriteMeshMaterial` whose uniform holds the size, anchor, rect, flip and atlas tile (`sprite_mesh/sprite_mesh_material.rs:36`). Swapping that material for a script one draws a 1×1 quad. Bevy's `SpriteMaterial<M>` keeps the sprite's fields through `MaterialExtension2d`, which takes static shaders again. A script sprite material needs an erased copy of `SpriteExt`, about as large as this change. learn's planets are quads, so it waits. `<material2d kind>` on an entity holding `Sprite` fails at the element, naming `<mesh2d shape="rectangle">` and a texture param as the way today.

### 8. The bench
`bench/material2d`, outside the workspace, as `bench/fullscreen-pass`: a release Bevy app drawing a `Camera2d` under `PixelCamera` zoom 3 to a 1920×1080 image target, headless, waiting for the GPU every frame, 600 frames after 120 warm-up. Three repo-made kinds stand in for the planet layers at the research's cost: `sea` (about 12 hashes a pixel, opaque), `land` (about 120, blend, with a river step), `cloud` (about 17, blend), and `ring` (about 16, blend). Integer hash, no `sin`. Arms: `off` (camera, no planets); `one` (a 200-art-px planet, three layers); `n20`, `n100`, `n200`, `n500` (48-art-px planets of three layers, a ring on every fifth, on whole world units, not overlapping). `n200` is recorded for learn-planets and decides nothing. Each planet's seed and size are instance params, so each kind is one asset. It prints the median and p95 frame time per arm, and the CPU time of the 2D main pass span from `RenderDiagnosticsPlugin` (Metal gives no GPU timestamps). `run.sh` sources `scripts/slot.sh`, prints the commit and `uptime`, and alternates the arms; `material2d-bench` and `material2d-bench-8` join `.abyme/processes.ts`. Three runs per arm, eight when arms are within noise. Results in `bench/material2d/results/README.md` and `runs.txt`.

Gate: `n100`'s median frame is under `off`'s plus 2 ms, in a run whose load stays under 10. A run at higher load is recorded but decides nothing.

## Risks / Trade-offs

- [Bevy's 2D specialise or queue skips entities whose material no `Material2dPlugin` registered] → spike 1.1 first, stop and report if so.
- [Blend layers interleaved by z draw one call each: 500 planets is about 1,500 draws] → the bench's CPU numbers show it; the doc says to put each layer kind in its own z band when planets do not overlap, so a kind's layers batch.
- [Quad-space art pixels shimmer on a quad at a fractional position] → the doc says to place quads on whole units, and offers the world-space form; tests cover both.
- [A 2D and 3D kind would share a name] → one namespace, refused at the second call.
- [The machine is never under load 10] → the results record the load, and the estimate stays in the doc marked as one; the gate is not met until a quiet run.

## Migration Plan

None: new API. `<material2d>` without `kind` is unchanged.

## Open Questions

None.
