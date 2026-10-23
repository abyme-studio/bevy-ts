# Bevy's four PBR texture features: cost (standard-material-fields, task 5)

Apple M5 Max, Metal, macOS 26.4, 2026-10-23. Machine shared with other builds (load 26 to 47).
Command: `bench/standard-material-features/run.sh 8` (the `smf-bench-8` process); raw lines in
`runs.txt`.

## Limits: passed

`wgpu::Limits::default()` allows 16 sampled textures and 16 samplers per shader stage.
`<bench> limits` sets every texture field the build has on one cube, on a device held to those
limits, and counts each pipeline's bindings across all its bind groups.

| Pipeline, fragment stage | Features off | Features on | Limit |
|---|---|---|---|
| `pbr_opaque_mesh_pipeline` textures / samplers | 9 / 9 | 18 / 18 | 16 / 16 |
| `pbr_prepass_pipeline` textures / samplers | 6 / 6 | 15 / 15 | 16 / 16 |

With the features on, wgpu rejects the opaque pipeline: "Too many bindings of type
SampledTextures in Stage FRAGMENT, limit is 16, count was 18". Bevy binds every texture field,
set or not (fallbacks), so this is every `StandardMaterial`, glTF ones included, on a device at
the default limits. `crates/bevy-ts-cli/tests/standard_material_limits.rs` fails the same way.
On this Mac's own limits the pipeline compiles.

## Frame and GPU time: no measurable difference

The arena's scene in Rust plus 100 distinct textured, normal-mapped materials, 3840x2160
offscreen, 600 frames per run after 240 warm-up, arms alternated, 8 runs each. Bevy records no
GPU timestamps on Metal, so "GPU synced" is the frame time with each frame waited out on the GPU.

| ms, 8 runs | Off mean (sd) | On mean (sd) |
|---|---|---|
| Frame | 7.48 (2.80) | 7.18 (3.25) |
| GPU synced | 11.85 (3.04) | 10.92 (2.99) |

The differences are well inside run-to-run noise.
