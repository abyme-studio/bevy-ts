## Why

Foliage is alpha-masked: a leaf card is a quad whose texture cuts the leaf out. After `script-material-passes`, a `{ mask }` script kind stays out of the prepass and the shadow pass by default, because Bevy's default prepass shader would draw the whole quad: square shadows and square ink. Agent town's trees need masked kinds that cut out in both passes. Kinds with a `vertex` shader (wind sway) have the same gap: their shadow is the unmoved mesh.

Stage 3 of the toon look, after `script-material-passes` lands.

## What Changes

- `material()` gains `prepassFragment` and `prepassVertex`, `.wgsl` imports as `fragment` and `vertex` are, used in the prepass and the shadow pass, as Bevy's `Material::prepass_fragment_shader` and `prepass_vertex_shader`. They see the kind's params and textures, as the main stages do.
- A `{ mask }` kind with a `prepassFragment` joins both passes by default; a kind with a `vertex` joins them by default when it has a `prepassVertex`. `shadows` and `prepass` still override.
- The new stages hot reload and report WGSL errors at the author's line, as `fragment` and `vertex` do, checked under the prepass pipeline's own shader defs.
- docs/materials.md: a foliage example (a masked leaf kind whose `prepassFragment` samples the same texture and discards).

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `script-materials`: two prepass stages, and the pass defaults that follow from them.

## Impact

- `crates/bevy-ts-material`: `kinds.rs` (two more stages, their reload and check), `render.rs` (`PrepassFragmentShader`/`PrepassVertexShader` on the kind's properties, the defaults), `wgsl.rs` (the prepass pipeline's defs recorded by `PipelineWatch`).
- `ts/material.js`, `ts/material.d.ts` and the `packages/bevy-ts` copies.
- Tests: a masked quad's shadow is the leaf, not the square; ink follows the cut-out; a swaying `vertex` kind's shadow moves with it; reload and error tests for the new stages.
- Performance: masked kinds in the shadow pass cost a fragment shader per shadow texel, as masked `StandardMaterial` does. Needs a go under the operator's delegation.
