## Context

- Research: docs/research/script-image-bytes.md (handles, the crossing, Bevy's upload, the bench).
- script-assets (branch `script-assets`, stacked on reflect-assets) gives the pattern:
  `crates/bevy-ts-cli/src/assets.rs` exports `Meshes` with `stands_for = "ResMut<Assets<Mesh>>"`;
  `Mesh.new` records in JS and `meshes.add` builds it in Rust; `getMut` returns a live object whose
  methods are host calls; refs are strong entries (`ScriptAssets::insert_strong`) carried by the
  `AssetRef` host kind (`ScriptRef<A>`).
- reflect-assets owns `ScriptAssets`, `HandleSlots` and `Image`'s hand-mapped `sampler` entry
  (its stage 3, with `Image.load(path, { sampler, is_srgb })`).
- A host slice is a view over the typed array's memory (`host.rs`, a `#[buffer]` op): V8 copies
  nothing; Rust copies once.
- Bevy 0.20: `Image.data` is `Option<Vec<u8>>`. `GpuImage::prepare_asset` keeps the GPU texture
  and view when the descriptor is unchanged and has `COPY_DST`, and writes the whole image with
  `write_texture`. Extraction clones the image when its usage keeps it in the main world.
- Script-material textures (`crates/bevy-ts-material/src/kinds.rs`, `world_textures` and
  `check_props`) take an asset path only.

## Goals / Non-Goals

**Goals:**
- Make, rewrite (whole or rect), read and remove an image from TS bytes, one crossing per call.
- Put its ref in any `Handle<Image>` slot and in script-material texture props.
- Cost near Rust's: a 4 MiB `setData` within 1.5 × the Rust floor plus 5 µs (bench: 91.0 against
  78.3 µs, met).

**Non-Goals:**
- Buffers (`ShaderBuffer`) and a buffer binding in script materials: the next change.
- 3D, array and cube images; mipmaps; compressed and wider formats; partial GPU upload.
- Sharing JS memory with Bevy's `Vec`: the image outlives the run and is extracted.

## Decisions

1. **`Images` beside `Meshes`.** `assets.rs` exports `Images` with
   `stands_for = "ResMut<Assets<Image>>"`, module `bevy-ts/assets`, in every engine. Host calls:
   `create(width, height, format, data) -> ScriptRef<Image>`, `set_data(ref, data)`,
   `write_rect(ref, x, y, cols, rows, data)`, `size(ref)`, `format(ref)`, `read_data(ref, out)`,
   `contains(ref)`, `remove(ref)`. The JS glue (`imagesClass` in `core.js`, as `meshesClass`) gives
   them Bevy's names. Argument names avoid `w`, `r` and `p`: the export macro on main breaks on
   them (fixed on script-assets).
   *Alternative:* `assets.add(Image, { data })` through reflect-assets. Rejected: JSON for 4 MiB.

2. **`Image.new` on the generated `Image`.** `bevy-ts/bevy`'s `Image` class (reflect-assets'
   `AssetClass<Image>`) gains `static new(size, format, data, options?)`, emitted by `types.rs`
   in place of the generated members, as animation-control does for `AnimationGraph`. One `Image`
   in TS: the same class is `Image.load`, `assets.set(img, ...)` and `AssetRef<Image>`.
   `TextureFormat` is a frozen object of the four formats, codes in JS, checked in Rust.
   *Alternative:* a second `Image` in `bevy-ts/assets`, as `Mesh`. Rejected: two `Image`s to
   import, and `AssetRef` types that do not match.

3. **One copy, no allocation on rewrite.** `create` copies the view into a fresh `Vec` (Bevy's
   `Image::new`). `set_data` checks the length and `copy_from_slice`s into `image.data`.
   `write_rect` checks the rect and copies row by row. Each goes through `Assets::get_mut`, which
   marks the asset changed; Bevy sends one `Modified` per frame. The checks run before `get_mut`,
   so a failed call marks nothing.

4. **The main world keeps the bytes.** `RenderAssetUsages::default()` (both worlds), so a rect
   has data to write into. The cost is Bevy's extraction clone per `Modified` (85.5 µs for 4 MiB).
   *Alternative:* `RENDER_WORLD` only, saving the clone. Rejected: the data is taken on
   extraction, so the next rect has nothing to write into.

5. **Sampler through reflect-assets.** `options.sampler` is written by reflect-assets' `Image`
   entry, the writer `Image.load` and `assets.set(img, { sampler })` use. If reflect-assets
   stage 3 has not landed when this builds, `options` waits for it (its own task) and
   `Image.new` takes no options until then.

6. **Texture props take refs.** In `kinds.rs`, `check_props` accepts a string or a ref object
   whose `type` is `bevy_image::image::Image`; `world_textures` resolves a ref through
   `ScriptAssets::handle_for(registry, TypeId::of::<Image>(), n)` and keeps the strong handle in
   the instance. The instance key stays the props' JSON, so equal refs share. bevy-ts-material
   already depends on bevy-ts; it turns on bevy-ts' `pbr` feature if `ScriptAssets` needs it.

7. **The bench moves to `bench/`.** docs/testing.md rule 8: a new bench lives outside the gate's
   build. `bench/script-image-bytes/` becomes a crate with path dependencies on `bevy-ts`,
   `bevy-ts-cli`, `bevy-ts-native` and `bevy-ts-v8`, its own target, run by the
   `image-bytes-bench` process; `tests/it/image_bytes_bench.rs` goes. It adds arm (f), the real
   `images.getMut(ref).setData` path, beside the research's bench-only param.

## Risks / Trade-offs

- [The whole image goes to the GPU per `Modified`] → Bevy's; docs say to batch writes in a frame
  and to write only when something changed.
- [JS producing every pixel costs 757 µs per 4 MiB] → docs say to keep one `Uint8Array` and change
  only what changed, then `setData`, or send a rect.
- [A 2048² image is 16 MiB per update] → measured by the bench at 1024² only; docs name the size.
- [script-assets or reflect-assets change their table or host kind before landing] → this
  change builds only after script-assets lands (tasks 0.1).

## Migration Plan

Additive. Texture props keep taking paths. Rollback is a revert.

## Open Questions

None. The lead decided the research's five (ordering, texture refs here, `writeRect`, buffers
later, the sampler through reflect-assets).
