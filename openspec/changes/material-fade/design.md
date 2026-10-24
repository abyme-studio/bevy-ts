## Context

`alpha.rs` (change `material-alpha`) fades a target by swapping its `StandardMaterial` for a copy
in `Blend`. Blend goes to `Transparent3d`: no depth write, sorted per mesh, skipped by the depth
prepass. A model's inner faces then draw in index order, the near-black in
`/tmp/civ-fade-1.png`. Its shadow stays full until alpha 0.05 (`PREMULTIPLIED_ALPHA_CUTOFF`).
`docs/research/material-fade.md` reads Bevy 0.20.0 and rejects the alternatives: Premultiplied,
Add, AlphaToCoverage, OIT, a blended depth prepass, driving `VisibilityRange`, `NotShadowCaster`
per mesh, `Propagate<NotShadowCaster>`.

Our setup: forward, no depth prepass, no TAA, cameras at 4× MSAA; render tests at `Msaa::Off`.

## Goals / Non-Goals

**Goals:**
- A faded opaque model draws as one solid at any alpha and any MSAA.
- Its shadow follows its alpha, with one owner: `alpha.rs`.
- `MaterialAlpha`'s API, its binding and its composition with scene-material stay as they are.

**Non-Goals:**
- Deferred rendering: under `DeferredPrepass` the gbuffer pass uses `StandardMaterial`'s deferred
  shader, so a faded opaque mesh draws solid and only its shadow fades. Forward only.
- Smooth (sub-1/16) fades, or hiding the pattern: that wants TAA, which we do not run.
- Fading a script material, or TSX children.
- A fade for `VisibilityRange` or any other Bevy fade.

## Decisions

1. One copy type, `FadeMaterial = ExtendedMaterial<StandardMaterial, Fade>`, for every original.
   `Fade` holds `alpha: f32` in a uniform (one `#[uniform]` field, bind group 2 at the
   extension's slot). Its `base` is a clone of the original. Why one type for every mode: a target
   always holds the same component while faded, so `alpha.rs` tracks one set of copies, and every
   mode gets the dithered shadow. Alternative: keep `StandardMaterial` copies for transparent
   originals. Rejected: two copy types, and their shadow would stay full.

2. The shader is `src/fade.wesl`: wesl, because Bevy 0.20's own shaders are wesl and its imports
   (`pbr_fragment`, `prepass::io`) resolve only from wesl. One fragment entry for the main pass and
   two for the prepass (with outputs, and depth only), picked by `@if`. Each computes
   `bayer4(frag_coord.xy)` (thresholds `(i + 0.5) / 16`) and discards where it is above `alpha`.
   The main pass skips the discard when the base's alpha mode is a blend mode (read from
   `pbr_input.material.flags`), and the Rust side already scaled the base's `base_color` alpha. The
   prepass always dithers. Built on Bevy's `pbr_input_from_standard_material` /
   `apply_pbr_lighting` / `main_pass_post_lighting_processing` and
   `prepass_sample_color_and_alpha_discard`, as `pbr.wesl` and `pbr_prepass.wesl` do. Why the
   prepass: the shadow pass specializes with `prepass_specialize`. Setting a prepass fragment shader
   makes `prepass_reads_material()` true, so Bevy binds the material in the depth-only and shadow
   draws; but Bevy gives an opaque base's depth-only pipeline no fragment stage at all (only
   `MAY_DISCARD` or prepass outputs add one). So `Fade::specialize` adds the shader's depth-only
   entry to any pipeline that has no fragment stage, and the shadow map loses the same texels; PCF
   softens it. The shader is inserted into `Assets<Shader>` under a fixed uuid handle at plugin
   build, not `embedded_asset!`: `specialize` gets no asset server, so it needs a handle it can
   name. Cost: no hot reload of `fade.wesl`, which only a bevy-ts developer edits.

3. `alpha.rs` owns the swap and the original. On fade: remove `MeshMaterial3d<StandardMaterial>`,
   insert `MeshMaterial3d<FadeMaterial>`. On restore: the reverse. `MaterialAlphaTargets` keeps
   `copies` keyed by the original's id and `originals` keyed by the copy's id, now
   `AssetId<FadeMaterial>`. `targets()` counts a mesh holding either type. The trigger stays
   `Changed<MeshMaterial3d<StandardMaterial>>`: a faded mesh holds none, so one appearing on it is a
   material given while faded, which `sync` copies and swaps again.

4. `scene.rs` asks `alpha.rs` for a mesh's material: `alpha::standard_of(world, mesh)` returns its
   `StandardMaterial` handle, or the original of its fade copy. When scene-material swaps a faded
   mesh it removes `MeshMaterial3d<FadeMaterial>` and keeps that original in
   `SceneMaterialTargets`; when it restores one under a faded root, alpha's next sync (it runs after
   `SceneMaterialSet` and sees the `Changed` `StandardMaterial`) fades it again. This drops today's
   rewrite of kept copies in `restore`. Why: one owner of "what was this mesh's material".

5. Shadows: nothing beyond the prepass dither. `alpha.rs` never touches `NotShadowCaster`.

6. Registration: `alpha::plugin` adds `MaterialPlugin::<FadeMaterial>` and inserts the shader under its handle.
   It sits in `ScriptMaterialRenderPlugin`, as `alpha::plugin` does today, so headless tests that
   add that plugin get it.

## Risks / Trade-offs

- [The 4×4 pattern shows without TAA; alpha steps in 1/16] → Accepted; it is Bevy's own
  cross-fade look. A larger map is a later change if it reads badly.
- [First fade of each original compiles a pipeline: a hitch] → Only on first use per original and
  pipeline key; the before/after frame task notes whether it shows.
- [Discard costs early-z on faded meshes, and their shadow draw runs a fragment shader] → Only
  faded meshes pay, only while faded; `MaterialAlpha`'s docs already say to remove it when not
  fading.
- [A reader of a faded mesh's `StandardMaterial` (CLI tests, edit-mode inspection) finds none] →
  Listed in tasks; edit mode shows `MeshMaterial3d<ExtendedMaterial<…>>` while faded.
- [Bevy's `pbr_fragment` API moves in a Bevy bump] → The shader is small and the render test
  catches it.

## Migration Plan

Lands as one change with its tests. Rollback is a revert: no saved data names the copy type.

## Open Questions

None.
