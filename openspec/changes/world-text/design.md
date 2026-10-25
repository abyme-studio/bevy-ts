## Context

Research (the `world-text-research` document, 2026-10-25) compared eight ways to draw text under a
`Camera3d` and picked one: Bevy's parley layout, an SDF atlas made from Bevy's own glyph raster, a
quad per glyph in a `Mesh3d`, a shared material. No new dependency. The lead took it.

What exists:

- `<text>`'s props and their parsers are `crates/bevy-ts-tsx/src/ui.rs` (`font_fields` at `:776`,
  `layout`, `line_height`). A `UiKind` there owns a tag's prop list, maps each prop to one
  component (`Part`), and gets bindings and prop-by-prop updates from `resolve.rs` and
  `reconcile.rs` (`Target::Ui`) for free.
- `<mesh>` and `<material>` are component intrinsics (`COMPONENT_INTRINSICS` in
  `packages/bevy-ts/runtime/jsx-runtime.js:18`); UI tags are entities of their own.
- `MaterialAlpha` is the pattern for an engine-made material: a component, a system, an
  `ExtendedMaterial<StandardMaterial, Fade>`, its shader `fade.wesl`, a prepass fragment that drops
  the same pixels (`crates/bevy-ts-material/src/alpha.rs`).
- Bevy 0.20 gives, all public: `TextPipeline::update_buffer` and `update_text_layout_info`
  (`bevy_text-0.20.0/src/pipeline.rs:72`, `:343`); `TextLayoutInfo.glyphs`, each with its Bevy atlas
  texture and rect; atlas images kept on the CPU (`font_atlas.rs:56`, `MAIN_WORLD`).

## Goals / Non-Goals

**Goals:**

- `<worldText size={0.5} outline={{ width: 0.1, color: "#000" }}>{() => name()}</worldText>` on
  an entity draws a label in the world, occluded, crisp, outlined, lit or unlit.
- The same fonts and fallback as `<text>`.
- 1,000 labels cheap: few materials, one atlas, no work on a still frame. A bench shows the cost.
- Stage 2: text along a path.

**Non-Goals:**

- Billboards (labels that turn to the camera) and constant screen size. Both are a transform
  written each frame; a later change, if a game asks.
- `<span>` rich text, underline, strikethrough, background and shadow in world text.
- MSDF. Single-channel SDF rounds corners only far past its raster size. If that shows, `fdsm`
  swaps in behind the same atlas and shader; that is a dependency ask, not this change.
- More than one atlas page; colour emoji in colour (they draw as one-colour shapes).
- `<text2d>` (the `script-2d` change) stays the 2D-camera answer. It shares the text props parser,
  nothing else.

## Decisions

1. **Code lives beside `MaterialAlpha`.** `crates/bevy-ts-material/src/world_text/`: `mod.rs`
   (components, `WorldTextPlugin`, the system), `layout.rs` (parley to glyph quads), `sdf.rs` (the
   distance transform), `atlas.rs` (the shared SDF atlas and its packer), `mesh.rs` (quads to a
   `Mesh`), `material.rs` (the extension), `world_text.wesl`. The crate gains Bevy's `bevy_text`
   feature; the engine already builds it through `bevy-ts-tsx`'s `bevy_ui`, so nothing new
   compiles. `WorldTextPlugin` is added wherever `ScriptMaterialPlugin` is. One owner: TSX only
   writes components; everything after them is this module's. Alternative rejected: a new crate
   (more build surface for one feature) or `bevy-ts-tsx` (a reconciler, not a renderer).

2. **`<worldText>` is a `UiKind` that is a component intrinsic.** `ui.rs` gains
   `UiKind::WorldText` (tag `worldText`), so its props, checks, bindings and prop-by-prop writes
   reuse the `<text>` machinery and `font_fields`/`layout`. Its parts: `text` writes `WorldText`;
   `font`, `weight`, `italic` write `TextFont` (its `font_size` is not used); `color` writes
   `TextColor`; `justify`, `linebreak` write `TextLayout`; `lineHeight` writes `LineHeight`
   (a multiple of `size` only: `px(n)` is refused); `size`, `letterSpacing`, `width`, `anchor`,
   `outline`, `lit` write `WorldTextStyle`. In `jsx-runtime.js` it joins `COMPONENT_INTRINSICS`,
   and its children go through `joined` into `text`, any element refused. `resolve.rs` refuses it
   beside `<mesh>` or `<material>` on one entity. Alternative rejected: an entity element like
   `<text>`: rich-text children cannot hold `<Transform>`, and a label placed by a level is an
   entity's component.

3. **Rust components are the interface.** `WorldText(String)` requires `WorldTextStyle`,
   `TextFont`, `TextColor`, `TextLayout`, `LineHeight`, `ComputedTextBlock`, `Transform`,
   `Visibility` and `NotShadowCaster`. `WorldTextStyle { size, letter_spacing, width, anchor:
   Vec2, outline: Option<(f32, Color)>, lit }`, `Default` and reflected. Bevy's UI and `Text2d`
   systems query `Text` and `Text2d`, so they leave these entities alone.

4. **Layout at one raster size.** A `PostUpdate` system, before `CalculateBounds`, takes labels
   whose components changed (or whose font was not loaded last time) and lays each out with its own
   `TextPipeline` (a `Local`), Bevy's `FontCx`, `LayoutCx`, `ScaleCx` and `FontAtlasSet`, at
   `FontSize::Px(48.0)` and scale factor 1. `width` becomes `TextBounds` of `width / size * 48`
   px, `letter_spacing` `LetterSpacing::Px(f * 48)`. Every layout length is then scaled by
   `size / 48`, Y flipped (parley's Y is down), and moved so `anchor` sits at the origin. A font
   still loading (`TextError::NoSuchFont`) marks the label to retry next frame. A count of labels
   laid out per frame is a resource, for the still-frame test and the bench.

5. **The SDF atlas is made from Bevy's raster.** `update_text_layout_info` rasterises each glyph
   into Bevy's bitmap atlas at 48 px. The SDF atlas keys a glyph by (Bevy atlas `AssetId`, rect):
   one entry per glyph Bevy rasterised. A new key reads the rect's alpha from the CPU copy, pads it
   by a 6 px spread, and runs Felzenszwalb's squared distance transform inside and out (corrected
   by coverage at the edge, so antialiased input gives sub-pixel distances). The signed distance
   over the spread maps to 0..1 in an `R8Unorm` cell, 0.5 at the edge. About 100 lines, no crate.
   Outline width is bounded by the spread: 6 / 48 = 0.125 em, the limit the spec states.

6. **One atlas, grown in place, UVs in texels.** The atlas is 4096 wide and starts 256 high
   (1 MB); a shelf packer places cells; when full it doubles the height, copying rows, up to 4096
   (16 MB, about 4,600 glyphs at 48 px). Mesh UVs are in texels and the shader divides by
   `textureDimensions`, so growing rebuilds no mesh. Past 4096 a glyph logs one error naming the
   limit and gets no quad.

7. **A mesh per label, replaced under its own handle.** Quads: position (x, y, 0), normal +Z, UV
   in atlas texels, `ATTRIBUTE_COLOR` the fill in linear RGBA, `u32` indices. A changed label puts
   its new mesh under the same `AssetId` (`Assets::insert`) and writes its `Aabb` itself, since
   Bevy computes `Aabb` only when it is missing. No sharing by props: a bound label would fill a
   cache with every value it ever showed. Render-world usage only.

8. **Few material instances, one shader.** `WorldTextMaterial = ExtendedMaterial<StandardMaterial,
   WorldTextExt>`. The base: white, `unlit = !lit`, `AlphaMode::AlphaToCoverage`, back faces culled.
   The extension: the atlas texture and sampler, `outline_color`, `outline` (in distance units),
   `spread`. The fragment samples the distance, takes coverage with `fwidth` for antialiasing,
   mixes outline colour and vertex colour, then either returns it (unlit) or runs Bevy's PBR
   lighting on it (lit). The prepass fragment discards below half coverage, as `fade.wesl` does.
   Instances are cached by (`lit`, outline) in a `WorldTextMaterials` resource; fill colour is per
   vertex, so 1,000 labels of one style draw with one material and batch. `AlphaToCoverage` goes to
   Bevy's alpha-mask phase and writes depth; without MSAA it is a hard edge at 0.5.

9. **Stage 2: a path is a per-glyph transform.** `mesh.rs` builds each quad from a glyph and a
   transform. Stage 1 passes the layout position. Stage 2's `path` (points in local space) and
   `normal` find each glyph's centre advance on the polyline by arc length and turn its quad to the
   tangent there. One line only. No new atlas or shader work.

10. **Tests by layer (docs/testing.md).** SDF, packer, layout and mesh in
    `crates/bevy-ts-material/tests/it/world_text.rs` under headless plugins; pixels in
    `world_text_render.rs` beside `alpha_render.rs`, offscreen; prop checks as `ui.rs` unit tests;
    TSX to components once, through V8, in `crates/bevy-ts-cli/tests/it/world_text.rs`; types in
    `elements.tsx`. The bench is `bench/world-text/`, out of the gate's build.

## Risks / Trade-offs

- [`update_text_layout_info` lays out for UI semantics (logical pixels, hinting)] → pass
  `FontHinting::Disabled` and scale 1; the layout test compares a label's width with the same
  string's `<text>` width at 48 px.
- [Far labels shimmer with no mipmaps on the atlas] → the far scenario is the check. It failed with
  one sample a pixel (16% over 10 sub-pixel moves at a 6 px em): a stroke thinner than a pixel is
  misjudged by one sample, which a mip chain does not fix. The main pass takes four samples a pixel
  on a rotated grid, each ramping over half a pixel (4%); no mip chain, so the atlas stays 16 MB.
- [`AlphaToCoverage` without MSAA aliases edges] → acceptable; Bevy's cameras default to MSAA 4.
  The render tests run both.
- [A label far bigger than 48 px on screen rounds its corners] → the trade research named. MSDF is
  the escape, behind the same atlas and shader.
- [Atlas growth re-uploads the whole texture] → it happens a handful of times in a game's life,
  at most 16 MB; the bench records the first-mount cost with growth in it.
- [Bevy's `FontAtlasSet` keeps every 48 px glyph raster too] → a second copy of each glyph in
  RGBA; the same as `<text>` at one size. Freeing it is a later change if memory shows.
- [A Bevy upgrade changes the text pipeline's API] → it is one file, `layout.rs`.
