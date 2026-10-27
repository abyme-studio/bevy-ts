## Why

A script material binds one sampler, Linear and Repeat, made at render startup (`crates/bevy-ts-material/src/render.rs:137-152`), as every texture's `<name>_sampler`. The image's own sampler never reaches the bind group. `Image.new(..., { sampler })` (`crates/bevy-ts-cli/src/assets.rs:489`), `Image.load(path, { sampler })` and `assets.set(img, { sampler })` all set `image.sampler`, and Bevy builds it into the `GpuImage`, but the material ignores it. civ-life's tile textures ask for ClampToEdge and get Repeat, so each tile's border blends with its far edge and the map shows cracks.

## What Changes

- Each texture binds its image's `GpuImage::sampler`, as Bevy's `AsBindGroup` does for `#[sampler]`. `ImageSampler::Default` gives the `ImagePlugin`'s default sampler, which Bevy has already resolved into the `GpuImage`.
- An unset texture binds the `FallbackImage`'s white texture with its sampler.
- A changed sampler (`assets.set(img, { sampler })`) rebinds every script material that holds the image, with no new material instance. A `setData` or `writeRect` that keeps the sampler rebinds nothing.
- A script material is prepared after the frame's images, so a material and its image changed in one frame bind the new sampler.
- `ScriptMaterialSampler` is removed: nothing else reads it.
- **BREAKING (behaviour)**: a texture whose image has the default sampler now clamps to the edge (Bevy's `ImagePlugin` default) instead of repeating. A script that tiled with `uv * n` and no sampler must give its image a repeating sampler, as `StandardMaterial` already requires.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `script-materials`: a texture samples with its image's sampler, and follows a change of it.

## Impact

- Operator's go: none needed. Nothing on the ask-first list: no new dependency, service or pattern (it is Bevy's `AsBindGroup` behaviour). Performance: a `setData` costs one hash lookup and a sampler compare per modified image; only a real sampler change re-prepares materials. The behaviour change above is what the lead should flag to civ-life and other games that relied on the old Repeat.
- Code: `crates/bevy-ts-material/src/render.rs` (bind `gpu_image.sampler` and the fallback's, drop `ScriptMaterialSampler`, prepare after `GpuImage`, the rebind system).
- World text is unchanged: `world_text/material.rs:24` derives `AsBindGroup` with `#[sampler(102)]`, which already binds the atlas's own ClampToEdge sampler (`world_text/atlas.rs:103`).
- Tests: new tests in `crates/bevy-ts-cli/tests/it/material_params_render.rs`, beside the existing image test.
- Docs: `docs/materials.md` (textures bullet, Tiling textures).
- Protocol: no change.
