## Context

World text places each glyph's distance field in one shared atlas, in the order glyphs are first
used. Mesh UVs are absolute atlas texels. Agent town's learn frame had two states: the same labels,
arriving in another order, put glyphs at other texels; f32 rounding of the interpolated UVs
changed with the position, and alpha-to-coverage under MSAA turned that into flipped samples at
letter edges.

## Decisions

- **Fix the order, not the shader.** A shader that filters from cell-local coordinates
  (`textureLoad`, four reads per tap) made a glyph draw the same anywhere, but cost frame time: on
  a quiet pair, 2.25 to 3.00 ms at 1920x1080 with 1,000 labels. Dropped. The atlas is instead made
  the same: layout and shader are unchanged, one hardware-filtered sample per tap.
- **Lay out, place, then mesh.** `lay_out` lays out every changed label first and asks the atlas
  for each glyph (`want`). `place` then makes the frame's new glyphs together, and only then are
  labels meshed. Cost: one sort over the frame's new glyphs; a frame with none sorts nothing.
- **Sort by the raster.** Bevy's `PositionedGlyph` carries no font or glyph id
  (`bevy_text-0.20.0/src/glyph.rs:15`), and its own atlas rect depends on Bevy's arrival order. The
  key is the raster: height (tallest first, so shorter glyphs fill its shelf), width, then the
  coverage bytes. Two rasters of the same pixels share one cell, so ties cannot swap.
- **Across frames, frame order.** Glyphs first used in different frames are still placed in frame
  order. Repacking on each new glyph would rebuild every label's mesh. A game that wants the same
  atlas every run spawns its labels in one frame; `ReadyHold` holds the shot, not when labels
  spawn, so it does not do that alone.

## Bench

`bench/world-text`, 1,000 labels at 1280x720, release, under a check slot. `layout_median_us` is
the median over 600 frames of the world-text system's time (from just before `WorldTextSystems`
to just after, `PostUpdate` on one thread). Arms: `still` (nothing changes), `churn` (100 labels
get new text each frame, no new glyphs), `fresh` (as churn, and two CJK glyphs never seen before
each frame, so `place` sorts and makes glyphs every frame). Main's `world_text` sources against
the fix, the same bench binary source, arms alternated, 8 runs each (3 runs were within noise).
Load average 40 to 74 over the run (7:55 to 8:36, 18 cores): a busy machine, as asked.

| Arm | main median (µs) | fix median (µs) | change | main's 8 runs (µs) |
| --- | --- | --- | --- | --- |
| still | 38.0 | 38.5 | +1.4% | 33 to 44 |
| churn | 869.5 | 915.8 | +5.3% | 404 to 1113 |
| fresh | 1242.2 | 1237.1 | -0.4% | 689 to 1488 |

Every change is inside the run-to-run spread. The atlas stays 4096x256 for the bench's 46 glyphs,
as on main (sorted shortest first it had grown to 512; tallest first fixed it). Raw lines:
`/tmp/wt-ab/runs-8.txt`.
