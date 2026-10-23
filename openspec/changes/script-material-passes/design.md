## Context

`ScriptMaterial` implements `ErasedRenderAsset` by hand (`crates/bevy-ts-material/src/render.rs`). Its `prepare_asset` registers only the three main-pass draw functions and sets `base_specialize`; `prepass_specialize`, `shadows_enabled` and `prepass_enabled` stay at their defaults (none, false, false). Bevy's queue systems skip a material whose flag is false, so script materials never cast shadows and never write the prepass. research@toon-look's proof (`docs/research/toon-look/proof.patch` at 9dcdd07) added the missing pieces and renders the result.

## Goals / Non-Goals

**Goals:**
- Script kinds draw in the prepass and shadow pass the way `MaterialPlugin<M>` draws a typed material.
- Flags that match Bevy's `enable_shadows`/`enable_prepass`, with defaults that never draw a wrong shadow.
- A TSX toon camera, tested.
- Numbers for 100 skinned characters.

**Non-Goals:**
- Custom prepass shaders (`prepassVertex`, `prepassFragment`). A follow-up.
- Deferred rendering for script materials.
- An outline post-process, or a toon or ink feature in the engine. Cel lighting and ink stay game WGSL.
- Script materials on glTF scenes from TS.

## Decisions

1. Copy Bevy's `prepass_specialize`. It is private in `bevy_pbr` 0.20, but every type it uses (`PrepassPipeline`, `PrepassPipelineSpecializer`, `SpecializedMeshPipelines`) is public. The copy is 20 lines and names its source; drop it if Bevy makes it public. Alternative: a typed shim `Material` to borrow `MaterialPlugin`'s path. Rejected: it adds a second asset type for no gain.

2. Register the draw functions Bevy registers for a forward material: `PrepassOpaqueDrawFunction`, `PrepassAlphaMaskDrawFunction` and `PrepassOpaqueDepthOnlyDrawFunction` (`DrawPrepass`/`DrawDepthOnlyPrepass` on `Opaque3dPrepass`/`AlphaMask3dPrepass`), and `ShadowsDrawFunction`/`ShadowsDepthOnlyDrawFunction` on `Shadow`. The deferred ones are left out (non-goal). Registering them is harmless when a flag is off: the queue checks the flag first.

3. Defaults. Bevy's default prepass shader knows nothing of a kind's `vertex` file or its alpha test. A kind with a `vertex` would cast the shadow of its unmoved mesh and fight its own depth in the main pass; a masked or blended kind would cast a solid shadow. So both flags default to true only for `alphaMode: "opaque"` with no `vertex`. Setting either to true explicitly takes Bevy's default shader, which is what a typed Bevy `Material` with no prepass shader gets. Alternative: refuse `true` on such kinds. Rejected: a vertex shader that only sets colours or UVs is fine in the default prepass, and Bevy allows it.

4. The flags live on `Kind` (resolved once from the declaration), and so in the kind's identity: a changed flag makes a new `Kind`, re-specialized as a changed `alphaMode` is today.

5. The camera is plain TSX elements: `<Camera3d />`, `<Projection>`, the prepass markers and `<Tonemapping>`. The test found two gaps. No prop could switch a reflected enum component's variant (`Projection` stayed `Perspective`; the insert failed with "no field Orthographic"), and `bevy-ts/bevy` declared Bevy's reflect-tier components as untyped handles, so any prop failed `tsc`. The fix is in the reflect-tier element path and the type emitter, for every such component. An enum component takes `value`, as the emitter already declared for project enums and as layout-tier `<Tonemapping value="None" />` reads: a variant name, or `{ Variant: fields }`. A newtype variant starts from its field's `Default`, else `FromWorld` (`OrthographicProjection` has only `FromWorld`, which gives the 3D defaults), else its current value, and `fields` sets only what it names, so its type is `Partial`. Rejected: one prop per variant (`<Projection Orthographic={...} />`), which reads well but departs from the `value` the emitter and layout-tier enums use; and a `projection` option on the `camera3d` recipe, a one-off.

6. The bench is an ignored release test in `crates/bevy-ts-material/tests/toon_bench.rs` (the crate already builds headless GPU tests; `bench/` holds separate workspaces), its results in `bench/toon/results/`: 100 `Fox.glb` instances (fetched from Bevy's repository at a pinned commit into a cache folder, not committed), each primitive's `StandardMaterial` swapped for a toon script material on `SceneInstanceReady`, playing their run animation, an orthographic camera, one shadowed directional light, rendered offscreen in release. Arms: no prepass (toon, shadows), and prepass plus ink. It records mean and p95 frame time, CPU and GPU where Bevy's diagnostics give them.

## Risks / Trade-offs

- Existing opaque script materials start casting shadows on the next build. This costs one depth draw per cascade per mesh and changes how scenes look (a glowing cube now casts a shadow). Mitigation: it matches `StandardMaterial`; `shadows: false` restores the old look. The examples are checked by eye in the headless render.
- The copied `prepass_specialize` can drift from Bevy's on upgrade. Mitigation: a comment naming the source, and the render test catches a broken prepass.
- Prepass-reading WGSL needs `DEPTH_PREPASS`/`NORMAL_PREPASS` defs, which the main-pass pipeline sets from the camera. The hot-reload check reuses the live pipeline's defs, so a file that reads the prepass checks under the right defs once drawn. Before first draw it is checked without them and may report an error the renderer would not; the existing "renderer's message" path covers that.
