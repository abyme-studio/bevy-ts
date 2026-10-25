# script-image-bytes: an Image from TS bytes

Change script-image-bytes, research stage. Goal: a TS script makes an `Image` (size, format,
`Uint8Array`), gets a handle a material samples, and later rewrites all of it or a sub-rect with
no reallocation. Bytes cross V8 once, with no call per pixel. First user: agent town's ownership
texture.

## 1. Handles today, and the pattern this fits

Two changes, both in flight, not on main:

- reflect-assets (`crates/bevy-ts/src/reflect_asset.rs`, branch `reflect-assets`) owns the
  `ScriptAssets` ref table. A ref is `{ $asset: n, type }`, `n` a `u64` from 1. `assets.add`,
  `get` and `set` build any reflected asset from JSON. Its `HandleSlots` hook in
  `reflect_value` turns a ref into the `Handle<T>` of any reflected slot: a TSX prop, or a field
  of another asset (`assets.add(StandardMaterial, { base_color_texture: img })`). `Image` is opaque
  to reflection: JSON is not a path for 4 MiB of pixels.
- script-assets (`crates/bevy-ts-cli/src/assets.rs`, branch `script-assets`, stacked on
  reflect-assets) adds strong refs (`insert_strong`, `drop`, `remove`), the `AssetRef` host kind
  (`ScriptRef<A>`), and `Meshes`, an exported param with
  `stands_for = "ResMut<Assets<Mesh>>"`. Vertex data crosses as typed-array slices, one call per
  attribute, copied once into the mesh's own `Vec`, in place when the length matches.

Image create and update is the `Meshes` pattern with `Image` in place of `Mesh`: an `Images`
param, `stands_for = "ResMut<Assets<Image>>"`, in `assets.rs`, returning `ScriptRef<Image>` from
`insert_strong`. No new pattern, no new host kind. It has to wait for script-assets to land.

The handle reaches a material two ways:

- `StandardMaterial` and any reflected material: the ref goes in `base_color_texture` (or any
  `Handle<Image>` field) through reflect-assets' hook. Works as soon as both changes land.
- A script material (`<material kind>`, `crates/bevy-ts-material/src/kinds.rs`): a texture prop
  takes an asset path only (`world_textures`, about line 502; "texture {p} takes an asset path",
  about line 527). It must also take a ref and resolve it through `ScriptAssets::handle_for`. That
  is a small change in bevy-ts-material, which owns the prop parsing, not a new pattern.

## 2. Bytes across once, and in-place mutation

- Crossing. An exported method's `&[u8]` argument is a view over the `Uint8Array`'s own memory
  (`host.rs`, "Slices cross in place"; the V8 op takes it as a `#[buffer]`,
  `crates/bevy-ts-v8/src/lib.rs` about line 62). V8 copies nothing. Rust copies once, into
  `Image.data`.
- In place. `Assets<Image>::get_mut` returns the image and marks it changed, so Bevy sends one
  `AssetEvent::Modified` per frame whatever the number of calls. `image.data` is a
  `Option<Vec<u8>>`: a whole update is one `copy_from_slice`; a sub-rect is one `copy_from_slice`
  per row (`width * 4` bytes at `((y + row) * W + x) * 4`). Neither allocates.
- Re-upload (`bevy_render` 0.20, `texture/gpu_image.rs`). On `Modified`, extraction clones the
  whole `Image` into the render world when its usage keeps it in the main world
  (`RenderAssetUsages::default()`); with `RENDER_WORLD` only it takes the data instead, and the
  main world loses it, so a later sub-rect edit has nothing to write into. `prepare_asset` keeps
  the GPU texture and its view when the descriptor is unchanged and has `COPY_DST`, and calls
  `write_texture` with the whole image. Bevy 0.20 has no partial upload: a 256×256 edit still
  sends 4 MiB to the GPU. The texture is not reallocated and the material's bind group stays valid.
- Size or format changed: Bevy makes a new texture. Images made from TS should keep the default
  usage (both worlds) and `COPY_DST` (Bevy's `Image::new` and `new_fill` set it).

## 3. Storage and uniform buffers

They fall out of the same path. `ShaderBuffer` (Bevy 0.20's name; there is no
`ShaderStorageBuffer`, `bevy_render/src/storage.rs`) is an asset whose data is bytes
(`ShaderBufferData::Initialized { data: AlignedVec, .. }`, `ShaderBuffer::new(data, usage)`).
Its `prepare_asset` keeps the GPU buffer when size, usage and label are unchanged, and calls
`write_buffer`. A `Buffers` param with `stands_for = "ResMut<Assets<ShaderBuffer>>"` is the same
code. A uniform buffer is the same asset with `BufferUsages::UNIFORM`. Binding one in a script
material is new: `bevy-ts-material`'s layout (`layout.rs`) binds a params struct and textures only,
so a `storage` or `buffer` binding kind is a layout change there. A `StandardMaterial` binds no
buffer. That half is a separate stage.

## 4. Bench

`crates/bevy-ts-cli/tests/it/image_bytes_bench.rs`, ignored, run through the
`image-bytes-bench` process in release. A bench-only exported param (`Pixels`, module
`bevy-ts/bench`) does `get_mut` and the copy, as `Images` would. 1024×1024 RGBA8, 4 MiB.
200 frames after 20 warm-up, each run in a fresh headless app. TS arms are timed by the system's
own run (`last_run_us`, whole µs) less an empty system's run (7.0 µs). Rust arms are timed inside
their system. Arms (a) and (b) were within noise at 3 runs (83.9 and 86.0 µs), so these are
8 runs per arm. Apple M5 Max, 2026-10-25.

| arm | what | median µs | p95 µs | GB/s |
|---|---|---|---|---|
| a | Rust `get_mut` + `copy_from_slice`, whole image (floor) | 78.3 | 100.4 | 53.5 |
| b | TS, one `write(bytes)`, whole image | 91.0 | 153.0 | 46.1 |
| c | TS, every pixel written by a JS loop, then `write(bytes)` | 757.0 | 1426.0 | 5.5 |
| d | TS, one `writeRect`, 256×256 (256 KiB) | 24.0 | 49.0 | 10.9 |
| e | Rust `Image::clone`, extraction with the main world kept | 85.5 | 106.8 | 49.0 |

- The crossing costs about 13 µs over the Rust floor for 4 MiB: one host call, and a copy at the
  same memory speed.
- Writing all 1M pixels in JS is the real cost (c: about 670 µs more than b). A script should
  keep its own byte array and change only what changed, then send it, or send the rect.
- (d) was 6 µs at 3 runs and 24 µs at 8: whole-µs timing and the empty run's subtraction make it
  noisy at this size. Either way it is far below a whole write.
- Bevy then costs about as much again on `Modified`: the extraction clone (e, about 85 µs), then
  `write_texture` of 4 MiB, which is not measured here and is the same from Rust or TS.
- A full 4 MiB update per frame from TS is about 0.2 ms of CPU on the main thread, plus the
  upload. For a texture written now and then, that is cheap.

## Recommended API shape

Bevy's names, as `Meshes` does:

```ts
import type { Assets, AssetRef, ResMut } from "bevy-ts";
import { Image, TextureFormat } from "bevy-ts/assets";

const owners = new Uint8Array(1024 * 1024 * 4);
let tex: AssetRef<Image> | undefined;

export function paint(images: ResMut<Assets<Image>>) {
  tex ??= images.add(Image.new([1024, 1024], TextureFormat.Rgba8UnormSrgb, owners));
  // ... change some bytes of `owners`, then one of:
  images.getMut(tex)!.setData(owners);                // whole image, same length: no allocation
  images.getMut(tex)!.writeRect(x, y, w, h, patch);   // packed rows, w*h*4 bytes
}

// A ref goes where a Handle<Image> goes:
const mat = assets.add(StandardMaterial, { base_color_texture: tex });
<material kind={Town} owners={tex} />   // once script-material textures take refs
```

- `Image.new(size, format, data)` crosses once in `images.add`. A 2D image only at first;
  `dimension` and `usage` are optional with Bevy's defaults (D2, both worlds).
- `setData` throws on a length that does not match the size and format; `writeRect` throws on a
  rect outside the image. Neither panics.
- Formats: the uncompressed 8-bit ones first (`R8Unorm`, `Rg8Unorm`, `Rgba8Unorm`,
  `Rgba8UnormSrgb`), then the others with a fixed bytes-per-pixel.
- Later stage: `ResMut<Assets<ShaderBuffer>>` with `ShaderBuffer.new(bytes)` and
  `getMut(...).setData`, and a buffer binding in script materials.

## The pattern

script-assets' `Meshes`: an exported param claiming Bevy's `ResMut<Assets<T>>`, strong refs in
reflect-assets' table, typed-array slices viewed in place and copied once into the asset's own
`Vec`. It needs no new pattern and no ask-first item of its own beyond what script-assets asked.

## Open questions

1. Ordering: this builds on script-assets (`Images` beside `Meshes`, the same
   `AssetRef`), which is stacked on reflect-assets. Is this change stacked on script-assets, or
   does it wait for both to land?
2. Script-material texture props taking a ref: owned by whoever owns `bevy-ts-material`
   (script-materials, archived). Done here, or asked of its owner?
3. Bevy's `Image` has no sub-rect method (`pixel_bytes_mut` is one pixel). Name it `writeRect`,
   or leave sub-rects to the script (`setData` of the whole array)? Both cost the same GPU upload.
4. Buffers in script materials (a layout binding kind): this change or a later one?
5. The sampler (nearest or linear, repeat or clamp) for an ownership map is per-image settings,
   which reflect-assets names as `Image.sampler` on its hand-map list. Does `Image.new` take a
   sampler too, or is that reflect-assets' `assets.set`?
