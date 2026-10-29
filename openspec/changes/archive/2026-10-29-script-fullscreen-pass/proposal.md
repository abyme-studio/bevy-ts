## Why

A 2D game grades its whole screen: dusk darkens and warms the valley as the day ends, fog drifts over it. Today that is Rust in a native crate (script-materials, "Render-world work beyond materials stays Rust"). A screen pass is data, as a material is: one WGSL file and a few params. Bevy 0.20's `FullscreenMaterial` runs one on a camera with no render-graph code of ours, and its schedule moves to `Core2d`. So a script can declare one, the way it declares a material.

## What Changes

- `bevy-ts/material` exports `pass(name, { params, fragment, order? })`: a fullscreen pass kind, its params laid out by WGSL uniform rules (as material params), its fragment a `.wgsl` import.
- TSX `<pass kind={Dusk} amount={() => dusk()} />` under a `Camera2d` entity runs that pass on the camera's image after the main pass, before upscaling. Its params may be bound: they live on the camera, not in a shared asset.
- The engine appends the screen texture, its sampler and the params struct to the author's WGSL. The struct starts with four fields the engine writes every frame: `time`, `texel` (screen pixels per world unit), `size` (the target in pixels) and `view` (the world point at the view's centre, as drawn), so an effect can snap to the art's texels and stick to the world.
- The screen is sampled nearest, and a pass leaves pixel art as crisp as it was: a pass that returns its input draws the same pixels as no pass, under `PixelCamera`'s whole zoom.
- `.wgsl` saves and schema changes hot reload as script materials do; WGSL errors name the pass and the author's `file:line:col`, and the camera keeps its last good pass, or none.
- A bench, `bench/fullscreen-pass`: one pass on against off over a 2D scene at 1920×1080, on test textures the bench makes.
- Follow-ups, out of scope: a second texture on a pass; render targets and `RenderLayers`; script `Material2d`; passes on a `Camera3d`; passes from a Rust app (it uses Bevy's `FullscreenMaterial` itself).

## Capabilities

### New Capabilities
- `script-passes`: fullscreen post-process passes on a `Camera2d`, declared in TS with a param schema and a `.wgsl` fragment, drawn by Bevy's `FullscreenMaterial` on `Core2d`.

### Modified Capabilities
- `script-materials`: "Render-world work beyond materials stays Rust" now excepts script passes; other passes, compute and render-world code stay Rust.

## Impact

- Frame cost: a pass reads and writes every pixel of the camera's target once. Estimate 0.15 to 0.6 ms of GPU time a frame at 1920×1080 per pass, by its shader; the operator approved this cost. A camera with no pass costs nothing new. `bench/fullscreen-pass` measures it, and its results land with the code.
- `crates/bevy-ts-material`: a new module `pass.rs` (kinds, slots, the frame fields, hot reload), reusing `layout.rs` and `wgsl.rs`; `ScriptMaterialPlugin` registers Bevy's `FullscreenMaterialPlugin` for four slot types. No new dependency: `FullscreenMaterial` is in `bevy_core_pipeline`, already built.
- `crates/bevy-ts-tsx`: the `<pass>` intrinsic. `packages/bevy-ts`: its types in `jsx-runtime.d.ts`.
- `bevy-ts/material` (`ts/material.js`, `ts/material.d.ts`): `pass()` and `PassKind`.
- Docs: `docs/materials.md` gains "Fullscreen passes"; `docs/2d.md` gains a pointer and a coverage row; `CHANGELOG.md` a line.
- `examples/shapes-2d`: a dusk pass on its camera.
- Tests: `bevy-ts-material`'s `pass.rs` and `pass_render.rs`, `bevy-ts-cli`'s `passes.rs`.
- Nothing is committed from Stardew Valley: tests, the example and the bench draw textures made in the repo or in code.
