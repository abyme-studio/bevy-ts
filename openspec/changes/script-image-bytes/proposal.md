## Why

A TS script cannot make an image from its own bytes or rewrite one in place. Agent town needs an
ownership texture: written every so often, sampled by a material. Today an image comes only from a
file, and `Image` is opaque to reflection, so `assets.add` cannot carry 4 MiB of pixels as JSON
(docs/research/script-image-bytes.md).

## What Changes

- `images: ResMut<Assets<Image>>` in a TS system is Bevy's image assets, the engine's `Images`
  export beside script-assets' `Meshes`. `images.add(Image.new(size, format, data, { sampler? }))`
  crosses once and returns a strong `AssetRef<Image>`. `images.get(ref)` reads its size, format
  and bytes. `images.getMut(ref).setData(bytes)` rewrites the whole image;
  `images.getMut(ref).writeRect(x, y, w, h, packedRows)` rewrites a rect. Neither reallocates.
  `images.remove(ref)` is `Assets::remove`.
- Bytes cross as one `Uint8Array`, viewed in place by Rust and copied once into the image's own
  `Vec`. No call per pixel.
- Formats: `R8Unorm`, `Rg8Unorm`, `Rgba8Unorm`, `Rgba8UnormSrgb`. 2D only.
- The sampler option is reflect-assets' `ImageSampler` shape, the one `Image.load(path, { sampler })`
  and `assets.set(img, { sampler })` take. No second spelling.
- A script material's texture prop (`<material kind={K} noise={...} />`) takes an image ref as well
  as an asset path.

Measured on the research bench (`crates/bevy-ts-cli/tests/it/image_bytes_bench.rs`, 1024×1024
RGBA8, 4 MiB, 8 runs of 200 frames, Apple M5 Max):

| what | median µs |
|---|---|
| Rust `get_mut` + one copy (floor) | 78.3 |
| TS, one `setData` of 4 MiB | 91.0 |
| TS, a JS loop writing every pixel, then `setData` | 757.0 |
| TS, one 256×256 `writeRect` | 24.0 |
| Bevy's extraction clone of the image on `Modified` | 85.5 |

Bevy 0.20 then re-uploads the whole image to the GPU on every `Modified`. It keeps the texture,
but has no partial upload, so a rect saves CPU only.

**Out of scope, next:** storage and uniform buffers from bytes (`ShaderBuffer`, a `Buffers` param,
a buffer binding kind in script materials), a later change on the same path. Also out: 3D and
array images, mipmaps, compressed formats, partial GPU upload.

## Capabilities

### New Capabilities

- `script-images`: images made and rewritten from TS bytes through `ResMut<Assets<Image>>`, and
  their refs.

### Modified Capabilities

- `script-materials`: a texture prop takes an image ref as well as an asset path.

## Impact

- Code: `crates/bevy-ts-cli/src/assets.rs` (the `Images` export), `crates/bevy-ts-cli/ts/assets.d.ts`,
  `crates/bevy-ts-cli/src/types.rs` (`Image.new` and `TextureFormat` on the generated `Image`),
  `crates/bevy-ts-v8/js/core.js` and `packages/bevy-ts/runtime/core.js` (the `Image` builder and
  live image), `crates/bevy-ts-material/src/kinds.rs` (texture props take refs),
  `packages/bevy-ts/bevy.d.ts` regenerated.
- Tests: `crates/bevy-ts-cli/tests/it/script_images.rs`, a render case in
  `crates/bevy-ts-cli/tests/it/materials.rs`, `crates/bevy-ts-material` unit tests for prop checks,
  `packages/bevy-ts/test/script-images.ts` for types. The research bench moves to `bench/`
  (docs/testing.md rule 8).
- Docs: `docs/assets.md` (images from bytes, the cost numbers), `docs/materials.md` (texture refs).
- Depends on: script-assets (`Meshes` pattern, strong refs, the `AssetRef` host kind), stacked on
  reflect-assets stage 1. The sampler option waits on reflect-assets stage 3 (`Image`'s sampler
  entry). Neither is on main.
- No new dependency, service or pattern. Protocol: none expected.
