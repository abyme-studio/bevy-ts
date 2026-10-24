## Why

`MaterialAlpha` turns an opaque copy into `AlphaMode::Blend`. Blend writes no depth and Bevy sorts
it per mesh, not per triangle, so a model's inner faces draw unsorted: a textured Synty model at
0.67 goes near-black (`/tmp/civ-fade-1.png`). It also keeps a full shadow until alpha 0.05, because
the shadow pass only discards a blended mesh below that. `docs/research/material-fade.md` has the
reading of Bevy 0.20.0 this rests on.

## What Changes

- A faded target draws with a dithered copy: Bevy's `ExtendedMaterial<StandardMaterial, _>` with a
  `MaterialExtension` of ours that discards pixels on a 4×4 ordered (Bayer) map where `alpha` is
  below the threshold, as Bevy's own `VisibilityRange` cross-fade does. The copy keeps the
  original's alpha mode, so depth stays written and a model fades as one solid. **BREAKING** for a
  reader of the mesh: a faded mesh holds `MeshMaterial3d<FadeMaterial>`, not
  `MeshMaterial3d<StandardMaterial>`, and an opaque original no longer becomes `Blend`.
- The same discard runs in the extension's prepass fragment shader, which Bevy's shadow pass uses.
  The shadow loses the same fraction of texels as the mesh: it follows `alpha`, and at 0 there is
  none. `alpha.rs` owns the rule; nothing else sets shadows for a fade.
- An already transparent original (`Blend`, `Premultiplied`, `Add`, `Multiply`) keeps today's
  look: its `base_color` alpha times `alpha`, its mode kept. Its shadow dithers like any other.
- Alpha steps in sixteenths, and the pattern shows (no TAA). Accepted.
- `scene.rs` reads a faded mesh's original through `alpha.rs`, so scene-material and
  `MaterialAlpha` still compose in either order.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `standard-materials`: `MaterialAlpha fades an entity's materials` changes how a copy draws
  (dithered, its mode kept, a second material type) and adds its shadow.

## Impact

- Code: `crates/bevy-ts-material/src/alpha.rs` (the extension, its WGSL, copies of a second type),
  `src/scene.rs` (a faded mesh's original), `src/lib.rs` (exports, the extension's plugin).
- Docs: `docs/materials.md`, the `MaterialAlpha` section and the scene-material note.
- Tests: `tests/it/alpha.rs` (copies are the new type), `tests/it/alpha_render.rs` (look, shadow,
  scene root), `bevy-ts-cli/tests/it/materials.rs` and `scene_material.rs` where they read a faded
  mesh's `StandardMaterial`.
- Ask-first, needs the operator's go before build:
  - New architectural pattern: the crate's first `MaterialExtension` and WGSL of its own outside a
    script material, and a second material type on faded meshes.
  - Performance: each distinct faded original specializes a new pipeline (a shader compile on its
    first fade); a fragment `discard` in the opaque pass costs early-z on faded meshes; their shadow
    draw runs a fragment shader where an opaque one was depth-only. Only faded meshes pay.
- No new dependency (Bevy's `ExtendedMaterial`), no service, no data, nothing in production.
