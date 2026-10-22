## Why

A script material draws in the main pass only. It casts no shadow and writes nothing to the depth and normal prepass, so a toon material can't receive cast shadows from other toon meshes and can't draw ink lines from the prepass. Agent town needs both for its painted, outlined look under an orthographic camera (docs/research/toon-look.md, proven by its three-panel render).

## What Changes

- Script materials join Bevy's prepass and shadow pass, as a typed `Material` does: the prepass and shadow draw functions, `prepass_specialize`, `shadows_enabled` and `prepass_enabled` on each kind's `MaterialProperties`. Both passes use Bevy's default prepass shader, which handles skinning and morphs.
- `material()` gains `shadows?: boolean` and `prepass?: boolean`, as Bevy's `Material::enable_shadows` and `enable_prepass`. Both default to true for an opaque kind with no `vertex`, and to false otherwise: Bevy's default prepass shader would draw a kind's moved vertices unmoved and its cut-outs solid. An author may set either to true to take that default anyway.
- Custom prepass shaders (`prepassVertex`, `prepassFragment`, as Bevy's `prepass_vertex_shader`/`prepass_fragment_shader`) are a follow-up, not this change.
- The camera: `bevy-ts/bevy` exports `Projection`, `DepthPrepass` and `NormalPrepass` (reflect tier) and `Tonemapping` (layout tier), so TSX should build an orthographic prepass camera already. A test proves it; only if it fails does the `camera3d` recipe gain `projection`, `prepass` and `tonemapping`.
- docs/materials.md gains a toon and ink section with the proof's shader as a starting point.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `script-materials`: kinds draw in the prepass and the shadow pass, with `shadows` and `prepass` flags.

## Impact

- `crates/bevy-ts-material/src/render.rs`: draw functions, a copy of Bevy's private `prepass_specialize`, the two flags.
- `crates/bevy-ts-material/src/kinds.rs`, `ts/material.js`, `ts/material.d.ts` (and the `packages/bevy-ts` copies): the two flags in the declaration.
- Tests: the three-panel headless render with assertions; a TSX camera test; a release bench of 100 skinned `Fox.glb` characters with a toon script material, with and without the prepass. `Fox.glb` is fetched from Bevy's repository at bench time, not committed.
- Performance: every existing opaque script material without a `vertex` now casts shadows and draws in any prepass a camera asks for. That is one extra depth draw per shadow cascade per mesh, as `StandardMaterial` already pays. This needs a go (the operator's delegation to lead@bevy-ts covers bevy work); the bench gives the numbers.
- Follow-ups: toon script materials on glTF scenes from TS (carrying base colour textures over), and custom prepass shaders.
