## Context

`ScriptMaterial::prepare_asset` (`crates/bevy-ts-material/src/render.rs`) builds the bind group by hand. For texture `i` it binds the image's `texture_view` at the texture binding and `ScriptMaterialSampler` (one Linear, Repeat sampler) at binding + 1. Bevy's `GpuImage` already carries the image's sampler: `prepare_asset` in `bevy_render-0.20.0/src/texture/gpu_image.rs:165` resolves `ImageSampler::Default` to `DefaultImageSampler` (the `ImagePlugin`'s) and builds a `Descriptor` with `create_sampler`.

The material is re-prepared only on its own `AssetEvent<ScriptMaterial>`. When an image is re-prepared, `GpuImage` keeps its texture view (same descriptor, `COPY_DST`) but makes a new `Sampler` every time. So `setData` works today without a rebind, and a sampler change is never seen by the material.

## Goals / Non-Goals

**Goals:**
- Bind each texture's own sampler, the fallback's for an unset texture.
- Rebind when an image's sampler changes, and only then.

**Non-Goals:**
- Rebinding when an image's texture is rebuilt (a new size or format). Script images keep their size (`script-images`), and an `assets.set` keeps the pixels; a path reload of a resized file is not handled here.
- Per-texture sampler props on `<material kind>`. The sampler belongs to the image, as in Bevy.
- World text: its `AsBindGroup` already binds the atlas's sampler.

## Decisions

1. Bind `gpu_image.sampler.clone()` beside `gpu_image.texture_view`, and `fallback.d2.sampler` beside `fallback.d2.texture_view`. This is what Bevy's `AsBindGroup` derive does for `#[sampler]`. `SamplerBindingType::Filtering` stays: the WGSL layout declares a filtering sampler, and every sampler a script image can be given (`ImageSamplerDescriptor`, no `compare`) is filtering. A Nearest sampler is valid in a filtering slot.

2. Remove `ScriptMaterialSampler` and its creation in `init_render_resources`. Nothing else reads it.

3. The rebind, main world. A system in `render.rs`, in `PostUpdate` after `AssetEventSystems`, keeps a resource `ImageSamplers(HashMap<AssetId<Image>, ImageSampler>)`: the sampler last seen for each image a script material holds. For each `AssetEvent::<Image>::Modified { id }`:
   - read the image's sampler; equal to the entry: nothing (the `setData` path);
   - else (different, or no entry yet): store it, and `get_mut` every `ScriptMaterial` whose `textures` holds `id`. That writes `AssetEvent::<ScriptMaterial>::Modified`, so Bevy re-extracts and re-prepares it with the same instance.
   `Removed` and `Unused` drop the entry. This follows `alpha.rs:183`, which reads `AssetEvent::Modified` and re-syncs what holds the asset. The common case costs one hash lookup and an `ImageSampler` compare (`PartialEq`, `bevy_image-0.20.0/src/image.rs:691`). The first `Modified` of an image with no entry rebinds its materials once; that is cheap and keeps the system free of an `Added` path.
   Rejected: comparing sampler ids in the render world and requeueing there. `ErasedRenderAsset` has no requeue hook, so it would copy Bevy's prepare queue.

4. Prepare after images. `ErasedRenderAssetPlugin::<ScriptMaterial, AfterImages>`, with a local `AfterImages` implementing `ErasedRenderAssetDependency` as `system.after(prepare_assets::<GpuImage>)`. Bevy's own impls only cover erased assets (`bevy_render-0.20.0/src/erased_render_asset.rs:157`), and `GpuImage` is a plain `RenderAsset`. Without it, a material and its image modified in one frame can prepare in either order and bind the old sampler for good.

## Risks / Trade-offs

- Behaviour change: a texture with the default sampler stops repeating. Mitigation: `docs/materials.md` says it, and the proposal flags it for civ-life and the examples. The build checks `examples/` and `templates/` for `uv *` tiling in script WGSL and gives those images a repeating sampler.
- One frame late: the rebind runs after the image's own `Modified` was extracted, so the frame of the change may draw the old sampler. The next one draws the new.
