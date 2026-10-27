## 1. Engine

- [x] 1.1 `crates/bevy-ts-material/src/world_text/atlas.rs`: `want` asks for a glyph, `place` makes a frame's asked glyphs sorted by raster, `cell` reads a placed one; equal rasters share a cell
- [x] 1.2 `crates/bevy-ts-material/src/world_text/layout.rs`: lay out every changed label, place the frame's glyphs, then mesh

## 2. Tests (docs/testing.md)

- [x] 2.1 `crates/bevy-ts-material/tests/it/world_text.rs`: labels spawned in one frame in two orders make the same atlas and UVs (fails before 1.2)

## 3. Docs

- [x] 3.1 `docs/world-text.md`: the frame's glyphs placed in their own order
- [x] 3.2 `CHANGELOG.md`: the atlas does not depend on the order a frame's labels were laid out in

## 4. Measure

- [x] 4.1 `bench/world-text`: the world-text system's time, still, churn and fresh-glyph arms, 8 runs per arm against main, in `design.md`
