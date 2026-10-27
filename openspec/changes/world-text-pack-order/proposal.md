## Why

Agent town's learn frame has two states, about 1 run in 12. Both are the same step: only
world-text letter edges differ (315 px, at most 31 levels, spread by bloom). World text placed each
new glyph in the shared atlas in the order labels were laid out, so labels that arrived in another
order put glyphs at other texels. Mesh UVs are absolute atlas texels; f32 rounding of the
interpolated UVs changes with the position, and alpha-to-coverage under MSAA turns that into
whole-sample flips at the edges.

## What Changes

- World text lays out a frame's changed labels first, then places that frame's new glyphs in the
  atlas together, sorted by their raster (tallest first, then width, coverage), then meshes the labels. The
  atlas, and every label's UVs, no longer depend on the order labels were laid out in one frame.
- Two glyph rasters of the same pixels share one cell.
- No shader change: one hardware-filtered sample per tap, as before.

## Capabilities

### Modified Capabilities

- `world-text`: the atlas does not depend on the order a frame's labels were laid out in.

## Impact

`crates/bevy-ts-material/src/world_text/` (`atlas.rs`, `layout.rs`), its headless test,
`docs/world-text.md`, `CHANGELOG.md`. One sort over a frame's new glyphs; a frame with none pays
nothing. Glyphs that first appear in different frames are still placed in frame order: a game whose
labels arrive over several frames gets one atlas only if they arrive in the same frames.
