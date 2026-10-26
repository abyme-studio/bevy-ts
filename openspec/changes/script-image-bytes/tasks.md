Build starts when script-assets has landed on main (it is stacked on reflect-assets stage 1).
Check with `git log main --oneline | grep -E "script-assets|reflect-assets"` and
`ls openspec/changes/archive | grep -E "script-assets|reflect-assets"`. Until then this change
holds its proposal and research only.

## 0. Before starting

- [x] 0.1 script-assets (and with it reflect-assets stage 1) is on main; rebase this branch on main
- [x] 0.2 Note whether reflect-assets stage 3 (`Image`'s sampler entry, `Image.load`) is on main; if not, task 2.4 waits for it and the rest goes ahead (it is: 2.4 is built on its `image_sampler`, through `reflect_asset::sampler_of_json`)
- [x] 0.3 Cargo only through `~/dev/abyme/scripts/gate-slot` (docs/check-slots.md); tests of the crates touched while working, no gate before landing

## 1. Rust: the `Images` export

- [x] 1.1 `crates/bevy-ts-cli/src/assets.rs`: `Images` (`stands_for = "ResMut<Assets<Image>>"`, module `bevy-ts/assets`) registered in `ScriptAssetsPlugin`; host calls `create`, `set_data`, `write_rect`, `size`, `format`, `read_data`, `contains`, `remove` (design decisions 1, 3, 4); every check before `get_mut`; no argument named `w`, `r` or `p`
- [x] 1.2 `assets.rs`: the four formats by code, their bytes per pixel, and messages naming size, format, length and rect; unit tests for the rect bounds and the length rule

## 2. Runtime and types

- [x] 2.1 `crates/bevy-ts-v8/js/core.js` and `packages/bevy-ts/runtime/core.js`: `imagesClass` (`add`, `get`, `getMut`, `remove`), the live image (`size`, `format`, `readData`, `setData`, `writeRect`), `Image.new` recording size, format, data and options, `TextureFormat`
- [x] 2.2 `crates/bevy-ts-cli/src/types.rs`: `Image.new` and `TextureFormat` on the generated `Image` (design decision 2); regenerate `packages/bevy-ts/bevy.d.ts` and `bevy.js` with `scripts/gen-bevy-types.sh`
- [x] 2.3 `crates/bevy-ts-cli/ts/assets.d.ts`: the `Images` interface and `LiveImage`
- [x] 2.4 `Image.new`'s `options.sampler`, written by reflect-assets' `Image` entry (design decision 5); waits for reflect-assets stage 3
- [x] 2.5 `packages/bevy-ts/test/script-images.ts` (`pnpm check`): `ResMut<Assets<Image>>` resolves to `Images`, `images.add` returns `AssetRef<Image>`, the ref fits `base_color_texture` and a texture prop, a mesh ref does not

## 3. Script-material texture refs

- [x] 3.1 `crates/bevy-ts-material/src/kinds.rs`: `check_props` accepts a path or an `Image` ref, with the spec's messages; `world_textures` resolves a ref through `ScriptAssets::handle_for` (design decision 6)
- [x] 3.2 `kinds.rs` unit tests: a ref passes the check, a mesh ref and a number fail naming kind and texture, equal refs give one instance key

## 4. Tests

- [x] 4.1 `crates/bevy-ts-cli/tests/it/script_images.rs` (`mod script_images;` in `main.rs`): the `script-images` scenarios under `MinimalPlugins` (from bytes, wrong length, whole, rect, rect outside, two writes one event, read back, remove)
- [x] 4.2 `crates/bevy-ts-cli/tests/it/material_params_render.rs` (not `materials.rs`: the render harness for script materials is there): a headless render of a `StandardMaterial` quad and a script-material quad sampling an image ref; `setData` with another colour is read the next frame (`common::render_app()`)
- [x] 4.3 The sampler scenario, with task 2.4

## 5. Bench

- [x] 5.1 Move `crates/bevy-ts-cli/tests/it/image_bytes_bench.rs` to `bench/script-image-bytes/` (a crate with path dependencies, its own target; docs/testing.md rule 8, design decision 7); remove its `mod` line; point the `image-bytes-bench` process at it
- [ ] 5.2 Add arm (f), `images.getMut(ref).setData` through `Images`; 3 runs per arm, 8 when arms are within noise; numbers into `docs/research/script-image-bytes.md` and `docs/assets.md`; target: (f) within 1.5 × (a) + 5 µs

## 6. Docs

- [x] 6.1 `docs/assets.md`: images from bytes (`Image.new`, `images.add/get/getMut/remove`, `setData`, `writeRect`), the formats, the costs (one copy, Bevy's whole-image upload, write only what changed)
- [x] 6.2 `docs/materials.md`: a texture prop takes a path or an image ref
- [x] 6.3 `docs/research/bevy-coverage.md`: the `Assets<T>` images row

## 7. Landing

- [ ] 7.1 `openspec validate script-image-bytes --strict`
- [ ] 7.2 Review, then archive and `abyme call land add`
