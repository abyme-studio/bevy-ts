## 1. Stage 1: components and layout (Rust only)

- [ ] 1.1 `crates/bevy-ts-material/Cargo.toml`: Bevy's `bevy_text` feature. `crates/bevy-ts-material/src/world_text/mod.rs`: `WorldText(String)` (its required components, design 3), `WorldTextStyle` (`Default`, reflected), `WorldTextPlugin`, a `LaidOut` count resource; `pub mod world_text` in `lib.rs`, its types re-exported
- [ ] 1.2 Add `WorldTextPlugin` beside `ScriptMaterialPlugin` in `crates/bevy-ts-cli/src/cli.rs:237` and `crates/bevy-ts-cli/src/types.rs:31`
- [ ] 1.3 `world_text/layout.rs`: the `PostUpdate` system before `CalculateBounds` (design 4): changed labels only, its own `TextPipeline`, 48 px, `width` to `TextBounds`, `letter_spacing` to `LetterSpacing::Px`, `FontHinting::Disabled`, retry while the font loads; out: glyphs in local units with anchor applied
- [ ] 1.4 `world_text/sdf.rs`: coverage to signed distance (Felzenszwalb, coverage-corrected), 6 px spread, `R8Unorm` bytes (design 5)
- [ ] 1.5 `world_text/atlas.rs`: the shared atlas, 4096 wide, 256 high at first, shelf packer, height doubled up to 4096, keyed by (Bevy atlas `AssetId`, rect); one error when full (design 6)
- [ ] 1.6 `world_text/mesh.rs`: glyphs to one `Mesh` (position, +Z normal, texel UV, colour, `u32` indices), inserted under the label's own `AssetId`, and its `Aabb` (design 7); a quad builder taking a per-glyph transform, for stage 2
- [ ] 1.7 `crates/bevy-ts-material/tests/it/world_text.rs` (a `mod` line in `main.rs`), headless: a filled disc's SDF crosses 0.5 within 0.5 px of its radius; the packer never overlaps cells and growth keeps them; "AB" at `size` 2 `bottom-left` starts at 0,0 with the spec's height; `width` wraps; bound text change keeps the handle and changes no other mesh; `Paris 中文 →` has no glyph id 0; one raster per distinct glyph over 1,000 labels; a still frame lays out 0; a Rust-spawned `WorldText::new("Paris")` has 5 quads; a label whose font loads a frame later draws then

## 2. Stage 1: material and shader

- [ ] 2.1 `world_text/material.rs`: `WorldTextExt` (atlas, sampler, outline colour, outline, spread), `WorldTextMaterial`, the `WorldTextMaterials` cache by (`lit`, outline) (design 8)
- [ ] 2.2 `world_text/world_text.wesl`: fragment (distance, `fwidth` coverage, outline mix, lit through Bevy's PBR or unlit) and prepass fragment (discard below half), as `fade.wesl`
- [ ] 2.3 `crates/bevy-ts-material/tests/it/world_text_render.rs`, offscreen like `alpha_render.rs`, MSAA 4 and off: a label draws in its bounds and not from behind; a red cube hides half of it; with `DepthPrepass`, red shows between glyphs; the close-up edge within 2 px; the far label's coverage within 10% over 10 sub-pixel moves; the outline band; lit dark and unlit white with no light; 1,000 labels of one style use one material

## 3. Stage 1: TSX

- [ ] 3.1 `crates/bevy-ts-tsx/src/ui.rs`: `UiKind::WorldText` (tag `worldText`), its prop list, `Part`s to the components of design 2; `size`, `width` positive, `outline.width` 0 to 0.125, `anchor` names and pairs, `lineHeight` a number only; unit tests for every refusal in the spec, each naming the prop
- [ ] 3.2 `resolve.rs`: `<worldText>` with `<mesh>` or `<material>` on one entity fails naming both; `reconcile.rs`: insert, update and remove go through `ui::` as other kinds do; remove drops the label's mesh and components
- [ ] 3.3 `packages/bevy-ts/runtime/jsx-runtime.js`: `worldText` in `COMPONENT_INTRINSICS`; its children through `joined` into `text`; any element child refused naming its location
- [ ] 3.4 `packages/bevy-ts/jsx-runtime.d.ts`: `WorldTextProps` (every prop `Bindable`, children strings, numbers and bindings), `worldText` in `IntrinsicElements`; `packages/bevy-ts/test/tsx/elements.tsx`: a valid label, `@ts-expect-error` for `<span>` inside, `textShadow`, `outline={0.1}` and `anchor="middle"`; mirror into `bench/dx/bevy-ts/jsx-runtime.ts` if it copies the intrinsics
- [ ] 3.5 `crates/bevy-ts-cli/tests/it/world_text.rs` (a `mod` line in `main.rs`), through V8 once: `<entity><Transform /><worldText size={0.5}>{() => name()}</worldText></entity>` mounts `WorldText`, `WorldTextStyle` and a `Mesh3d`; a changed `name` rewrites only `WorldText`; a refused prop fails the load with the file and line
- [ ] 3.6 Regenerate `packages/bevy-ts/bevy.d.ts` (`scripts/gen-bevy-types.sh`): `WorldText` and `WorldTextStyle` typed

## 4. Stage 1: bench, docs, example

- [ ] 4.1 `bench/world-text/` (`Cargo.toml` with a path dependency on `bevy-ts-material`, `src/main.rs`, `run.sh` sourcing `scripts/slot.sh`): 1,000 labels from Rust (distinct names, two colours, one outline) at 1280×720 offscreen; arms: 0 labels, 1,000 still, 1,000 with 100 texts changed a frame; records first-mount ms (atlas growth included), frame time median and p95 over 600 frames after 240 warm, labels laid out per frame, material count; `bench/world-text/results/README.md` with the machine and load
- [ ] 4.2 New `docs/world-text.md`: the element, every prop with units and default, the Rust components, what fails the load, limits (atlas size, outline width, corners when magnified, no billboard), how it works, the bench's numbers; link from `README.md`, the Text section of `docs/ui.md` and `docs/fonts.md`
- [ ] 4.3 `examples/cube/src/game.tsx` (or the closest 3D example): a label over the cube with an outline and a bound counter
- [ ] 4.4 Gate: `cargo test -p bevy-ts-material`, `-p bevy-ts-tsx`, the cli `world_text` test, the TS type tests, through the gate slot; `protocol/bevy-ts.openrpc.json` unchanged

## 5. Stage 2: text along a path (after stage 1 lands)

- [ ] 5.1 `WorldTextStyle` gains `path: Vec<Vec3>` and `normal: Vec3`; `ui.rs` takes `path` (at least two points) and `normal`, refuses `path` with `width` or a line break, naming the props; stage 1's refusal of `path` goes
- [ ] 5.2 `world_text/mesh.rs`: each glyph's centre advance placed on the polyline by arc length, its quad turned to the tangent and `normal`; `justify` start, centre, end along the path; past the end along the last tangent (design 9)
- [ ] 5.3 `world_text.rs` test: `ROME` on a 64-point half circle of radius 4, centre placement and tangent within the spec's bounds; `path` with `width` fails; `jsx-runtime.d.ts` and `elements.tsx` gain `path` and `normal`
- [ ] 5.4 `docs/world-text.md`: the path section; `examples/cube`: a curved label
