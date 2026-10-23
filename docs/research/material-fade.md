# Fading an opaque model, and its shadow (material-fade)

Read 2026-10-24 from the Bevy we pin (0.20.0, the registry sources). The question: `MaterialAlpha`
turns an opaque copy into `AlphaMode::Blend`, which makes a textured glTF near-black at 0.67 and
leaves its full shadow. What does Bevy itself do, and what should we?

## What Bevy does

- Blend, Premultiplied, Add and Multiply go to `Transparent3d`, sorted per mesh, not per triangle,
  with depth write off (`bevy_pbr/src/render/mesh.rs`, `specialize`, the `BLEND_*` arms). The
  depth prepass skips them (`prepass/mod.rs`, `RenderPhaseType::Transparent => continue`). So
  inner faces and overlapping parts of one model draw unsorted, in index order: the near-black.
  Bevy has no depth-prepass-for-blended option.
- Bevy's own fade of an opaque mesh is `VisibilityRange`'s cross-fade: the mesh stays opaque, and
  `pbr_functions::visibility_range_dither` discards pixels on a 4×4 ordered (Bayer) map, 16 levels.
  It runs in the main pass and the prepass (`pbr.wesl:52`, `pbr_prepass.wesl:51`). Its level comes
  from camera distance only, so it cannot be driven by an alpha.
- `AlphaToCoverage` stays in the opaque phase with depth write on. With MSAA (the `Msaa` default is
  `Sample4`) alpha becomes sample coverage, 5 levels at 4×. With `Msaa::Off` it falls back to a
  mask at `alpha_cutoff` (`material.rs`, `alpha_mode_pipeline_key`): the fade pops.
- Order-independent transparency (`OrderIndependentTransparencySettings` on the camera) sorts
  blended fragments, but panics with MSAA above 1 (`core_pipeline/src/oit/mod.rs`, `check_msaa`).
- The shadow pass (`render/light.rs`, `specialize_shadows`) never sets `VISIBILITY_RANGE_DITHER`:
  Bevy does not dither shadows. A Blend, Add, Premultiplied or ATC material in the shadow pass
  only discards below alpha 0.05 (`PREMULTIPLIED_ALPHA_CUTOFF`), so a faded mesh casts a full
  shadow until it is nearly gone.
- A mesh skips the shadow pass when its material type has `enable_shadows() == false` (per type,
  not per instance) or it has `NotShadowCaster` itself (`SHADOW_CASTER` flag, read on the mesh
  entity in `render/mesh.rs`). Nothing propagates it: `NotShadowCaster` on a scene root reaches no
  mesh. Bevy has `HierarchyPropagatePlugin<C>` / `Propagate<C>` (`bevy_app/src/propagate.rs`), but
  registers it only for UI, never for `NotShadowCaster`.

Our setup: forward rendering, no depth prepass, no TAA, cameras at the default 4× MSAA; the render
tests use `Msaa::Off`.

## Recommendations

1. Fade by dither, as `VisibilityRange` does: the copy stays in its original alpha mode and a
   `MaterialExtension` on `StandardMaterial` (`ExtendedMaterial`) discards pixels where
   `alpha < bayer(frag_coord)`, in its fragment and prepass fragment shaders. Why: depth stays
   written, so a model fades as one solid with no sorting, under any MSAA, at Bevy's own cost.
   Rejected:
   - Blend (today): no depth write, unsorted inner faces.
   - Premultiplied / Add: the same transparent phase and the same fault.
   - AlphaToCoverage: pops with `Msaa::Off`, 5 levels at 4×, full shadow.
   - OIT: panics with MSAA, a camera-wide cost for one faded model.
   - A depth prepass for blended meshes: Bevy has none; a second depth-only draw per mesh is ours
     to build and keep.
   - Driving `VisibilityRange`: its level is camera distance, and the shadow pass ignores it.
2. The shadow fades with the same dither: the prepass fragment shader is the shadow pass's
   (`specialize_shadows` uses `prepass_specialize`; a material with a prepass fragment shader is
   never depth-only), so the shadow map loses the same fraction of texels and PCF softens it. Why:
   no second switch, the shadow follows the alpha. Owner: `alpha.rs`, the one place that knows a
   target's alpha. Rejected:
   - `NotShadowCaster` on each target below a threshold: the shadow pops, and `alpha.rs` must
     remember which meshes already had one.
   - `Propagate<NotShadowCaster>` from the root: a second owner of the same meshes, not registered
     by Bevy, and its removal would strip an authored `NotShadowCaster`.
   - Blend's 0.05 cutoff (today): full shadow at any visible alpha.

The cost of 1: a faded target holds `MeshMaterial3d<ExtendedMaterial<StandardMaterial, Fade>>` in
place of its `StandardMaterial` handle. `alpha.rs` then tracks copies of a second type, and
`targets()`, the `Changed<MeshMaterial3d<StandardMaterial>>` trigger and `scene.rs`'s kept
originals must see a faded mesh as still the root's. The pattern at 4×4 shows without TAA; alpha is
quantised to 1/16.

## Tests

- Look, headless, in `crates/bevy-ts-material/tests/it/alpha_render.rs`'s app (offscreen target,
  white clear, `Msaa::Off`, unlit): a red cube holding a smaller black cube, under
  `MaterialAlpha(0.5)`. Today's Blend shows dark pixels through the red; dither shows only red or
  white. Assert no pixel in the cube's rect is near black, and its red fraction is 0.5 ± 1/16.
- Shadow, same app, lit: a directional light with shadows, a white ground plane, three cubes above
  it at alpha 1, 0.5 and 0. Sample the ground under each: the 0.5 shadow is lighter than the 1,
  darker than the 0, and the 0 equals the unshadowed ground.
- A scene root: the same as the look test with `ship.gltf` under a faded root, so scene meshes take
  the dithered copy and their shadow follows.
- Before/after frame: the scene of `/tmp/civ-fade-1.png` (a Synty model at 0.67), same camera,
  once on today's Blend and once on the dither, as a pair for review. It is evidence, not a golden
  file (`docs/testing.md`, rule 4).
