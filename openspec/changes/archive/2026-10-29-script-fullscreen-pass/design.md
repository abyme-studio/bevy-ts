## Context

Script materials (change `2026-10-22-script-materials`, `docs/materials.md`) give a mesh a WGSL file and a param schema declared in TS, with no Rust. Whole-screen effects were left to the native crate. Bevy 0.20 ships `FullscreenMaterial` (`bevy_core_pipeline-0.20.0/src/fullscreen_material.rs`): a component on a camera, a `ShaderType` uniform, one fragment shader, run as a fullscreen triangle that reads the view's image and writes the other half of its ping-pong (`post_process_write`). Its trait defaults to `Core3d`; `schedule()` and `schedule_configs()` move it (line 80).

What it fixes per Rust type: one shader (`fragment_shader()`, a static `ShaderRef`), one bind group layout (screen texture, a default sampler, one uniform of `T`'s size, all group 0), one system in the render schedule. A script kind is declared at runtime, so the engine needs Rust types that a kind can take at runtime.

The use is 2D pixel art (`docs/2d.md`, Pixel camera): dusk and fog grading over a valley, drawn at whole zoom with nearest sampling.

## Goals / Non-Goals

**Goals:**
- A script declares a fullscreen pass on a `Camera2d` with a `.wgsl` fragment and params, plus engine-written time and view fields, and no Rust.
- Pixel art stays crisp under a pass.
- Hot reload and errors as script materials have them.
- A measured frame cost.

**Non-Goals:**
- A second texture on a pass (noise, a LUT).
- Render targets, `RenderLayers`, or passes on one layer.
- Script `Material2d`.
- Passes on a `Camera3d`, and a Rust API for script passes.
- Any JS in the render world.

## Decisions

### 1. Bevy's `FullscreenMaterial`, on four slot types
`pass.rs` defines `ScriptPass<const SLOT: u8>`, a camera component holding 256 bytes (`[Vec4; 16]`), and implements `FullscreenMaterial` for slots 0 to 3: `schedule()` is `Core2d`, `schedule_configs` puts slot `n` in `Core2dSystems::PostProcess` after `tonemapping` and after slot `n - 1`, and `fragment_shader()` is a fixed UUID handle per slot. `ScriptMaterialPlugin` adds `FullscreenMaterialPlugin::<ScriptPass<n>>` for each.

A kind takes a slot. Its WGSL, with the declarations appended (decision 3), is written into its slot's shader handle, so the pipeline Bevy built for that slot draws the kind's shader. The uniform is always 256 bytes; the WGSL struct the kind declares is that size or smaller, which the layout's minimum binding size accepts. A `<pass>` writes the kind's encoded bytes into `ScriptPass<slot>` on the camera.

- After tonemapping: a 2D camera grades the colours it shows. `Camera2d` defaults to no tonemapping and an `Rgba8UnormSrgb` target; Bevy's specializer keys on the target format, so an HDR camera works too.
- Four kinds at once. Each slot is one plugin and one pipeline when used; a camera without a slot's component skips its system (Bevy's `ViewQuery`), so unused slots cost nothing. A fifth live kind fails the load at its `pass()` call naming the limit. Raising it is a constant.
- 256 bytes, as instance params are capped: sixteen `vec4`s is a lot of grading.
- *Beat: our own render system with a layout per kind.* It is a copy of Bevy's 250 lines (pipeline, bind group cache, ping-pong) we would keep in step with Bevy; the brief and the plugin's own docs point at `FullscreenMaterial`.
- *Beat: one slot type, one pass a camera.* Dusk and fog would have to share one file, and two cameras could not run different kinds.

### 2. Order is the kind's `order`, then its name
Slot `n` runs before slot `n + 1`, so the slot order is the run order. Each applied generation sorts its live kinds by `order` (default 0), then by name, and gives them slots 0 up. When that changes a kind's slot, the apply moves its shader into the new slot's handle and moves every camera's component to the new slot type, in the same world update, so no frame draws a kind's bytes under another kind's shader.

- *Beat: slots by declaration order.* Hot reloads that add or drop a kind would reorder passes as a side effect of editing.
- *Beat: order on the `<pass>` element.* Slot order is global, so a per-camera order could not hold on two cameras that disagree.

### 3. Generated declarations, appended
As materials do (script-materials decision 1), the engine appends after the author's last line:

```wgsl
struct DuskParams { time: f32, texel: f32, size: vec2<f32>, view: vec2<f32>, amount: f32, tint: vec4<f32> }
@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> params: DuskParams;
```

The author's file imports `bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput` and writes `@fragment fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32>`. `layout.rs` lays the struct out: the four frame fields first, at fixed offsets 0, 4, 8 and 16, then the kind's params by WGSL uniform rules. `time`, `texel`, `size`, `view`, `screen`, `screen_sampler`, `params` and the struct's name are reserved; a param named one fails the load at `pass()`, as materials refuse colliding names. The `wgsl.rs` check and its error mapping are reused unchanged: author lines keep their numbers.

### 4. The frame fields, written each frame in the main world
A system in `PostUpdate`, after the pixel camera's snap (`Pixel2dSystems`) and Bevy's camera update, writes into every `ScriptPass<n>`:

- `time`: `Time`'s elapsed seconds, wrapped as Bevy's `globals.time` wraps, so a paused game holds a drifting fog.
- `size`: the camera's physical target size in pixels.
- `texel`: physical pixels per world unit, `size.x` over the orthographic area's width. Under `PixelCamera` at zoom 2 on a 1x target it is 2; on a 2x display, 4.
- `view`: the world point at the target's centre as drawn, the camera's position after the snap's `viewport_origin` shift. A shader gets a pixel's world point as `params.view + (in.uv - 0.5) * vec2(1, -1) * params.size / params.texel`.

`ExtractComponentPlugin` extracts every camera's component each frame and `UniformComponentPlugin` uploads it, so there is no change tracking to keep.

### 5. Crisp pixel art
The pass runs at the target's resolution, after the sprites drew at whole zoom. Bevy's pass sampler is `SamplerDescriptor::default()`, nearest, so a shader reading `textureSample(screen, screen_sampler, in.uv)` reads each pixel whole, and one shifting UVs moves whole pixels. A pass that returns its input draws the same pixels as no pass: the render test holds this at `PixelCamera` zoom 2. An effect of its own (fog noise) snaps to the art by flooring the world point (decision 4) to whole world units; the doc's fog example does this.

### 6. `<pass>` lives on the camera, so it binds
`<pass kind={Dusk} amount={0.6} />` is a child element of a `Camera2d` entity, as `<material>` is of a mesh entity. It inserts `ScriptPass<slot>` with the kind's encoded bytes; a changed or bound prop re-encodes into the component that frame. Unlike material params, pass params may be bound (`amount={() => dusk()}`): they are per camera, there is no shared asset to key. On an entity without `Camera2d`, or a second `<pass>` of the same kind on one camera, the load fails at the element. Removing the element removes the component. `ScriptPass` is not reflected, so levels never save it: a pass is TSX.

- *Beat: a shared asset per props, as materials.* Grading changes every frame of a dusk; per-camera bytes cost one 256-byte upload a frame either way.

### 7. Hot reload and errors, as materials
Declarations go through a `passes` section of the generation manifest, checked and applied as the `materials` section is (`kinds.rs`). A `.wgsl` save (`on_ts_files_changed`) checks the file with the pass's appended declarations and Bevy's fullscreen imports, then replaces its slot's shader in place: Bevy's pipeline cache re-specialises, no JS runs, no generation starts. A failed check keeps the last good shader; with none, the slot's pipeline is not ready and Bevy's system skips it, so the camera draws as with no pass. A changed schema re-encodes each camera's bytes by param name; a schema whose shader no longer checks waits, as a material's does. Renderer failures go through `PipelineWatch` with versions, as materials' do.

### 8. The bench
`bench/fullscreen-pass`, outside the workspace (docs/testing.md, rule 8), as `bench/pixel-2d` is: a release Bevy app rendering a `Camera2d` with `PixelCamera` zoom 3 to a 1920×1080 image target (headless, nothing on screen), 2,000 sprites from a 16×16 atlas the bench fills in code, arms `off`, `passthrough` and `grade` (a dusk-like shader reading the screen once and writing `time`-driven tint). It reads `RenderDiagnosticsPlugin`'s GPU time for the pass span (`fullscreen_material_pass<...ScriptPass<0>>`) and the frame time, median and p95 over 600 frames after 120 warm-up, arms alternated. `run.sh` runs in a check slot; a `fullscreen-pass-bench` entry joins `.abyme/processes.ts`; results go in `bench/fullscreen-pass/results/README.md`.

## Risks / Trade-offs

- [Metal may not give GPU timestamps for the pass span] → the bench reports frame time per arm too, and the results say which number decides.
- [A slot reassignment moves components and shaders in one apply; a frame between could draw a kind's bytes under another's shader] → both happen in the apply's world update, before extraction; `pass.rs`'s test reorders two kinds and checks each camera's bytes follow their kind.
- [A fifth kind] → refused with the limit named; a project needing more raises one constant.
- [`FullscreenMaterial`'s API changes in a later Bevy] → `pass.rs` is the one place that implements it.
- [A shader that offsets UVs by a fraction of a pixel still reads whole pixels, so a slow shimmer steps] → that is the crisp look; the doc says so.

## Migration Plan

None: new API. The script-materials requirement that kept all passes in Rust narrows to "other passes".

## Open Questions

None.
