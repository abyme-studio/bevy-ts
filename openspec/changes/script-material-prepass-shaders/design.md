## Context

`script-material-passes` registers the prepass and shadow draw functions for every kind, with Bevy's default prepass shader, and keeps `{ mask }` and `vertex` kinds out of both passes by default. Bevy's `PrepassPipelineSpecializer` already uses `PrepassFragmentShader`/`PrepassVertexShader` from a material's properties when present, and runs the fragment for `MAY_DISCARD` even with no prepass targets (`prepass/mod.rs`, `fragment_required`).

## Goals / Non-Goals

**Goals:** masked and vertex-moved kinds correct in both passes, following Bevy's API. **Non-Goals:** running the main `fragment` in the prepass (its view bindings differ there); deferred shaders.

## Decisions

1. Author-written prepass stages, as Bevy's own API, not an automatic alpha test. A script kind's alpha is whatever its WGSL computes; bevy-ts cannot know which texture or channel is the mask.
2. The stages reuse `Stage` in `kinds.rs` (file, handle replaced in place, version). `PipelineWatch` records defs per shader handle, so the prepass pipeline's defs are recorded under the prepass stage's handle with no new mechanism.
3. Defaults extend stage 1's rule: a pass is on by default exactly when every stage it would draw with is the author's or matches the default's assumptions.

## Risks / Trade-offs

- An author must keep `fragment` and `prepassFragment` cutouts in step. Docs show sharing one WGSL function through a module import.
