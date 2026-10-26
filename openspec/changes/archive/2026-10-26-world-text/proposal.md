## Why

A game drawn by a `Camera3d` has no text in the world: city names on a map, a unit's name over its
head, a sign. `<text>` is screen UI. Bevy's `Text2d` does not draw under a `Camera3d`: it queues
only into the 2D transparent phase (`bevy_sprite_render-0.20.0/src/render/mod.rs:505`), and its
bitmap atlas blurs close up and shimmers far away. Research: the `world-text-research` document
(abyme documents, bevy-ts), 2026-10-25.

## What Changes

- A new intrinsic, `<worldText>`: a component of its `<entity>`, as `<mesh>` is. Its children
  (strings and bindings) are its text. It lies in the entity's local XY plane, facing +Z, placed
  by the entity's `Transform`.
- Props: `<text>`'s font props (`font`, `weight`, `italic`, `justify`, `linebreak`), parsed
  by the same code; `lineHeight` as a multiple of `size`; `size` in world units; `color`; `letterSpacing` as a
  fraction of `size`; `width` (wrap width, world units); `anchor`; `outline` (`{ width, color }`);
  `lit`. Every prop is bindable, as `<text>`'s are.
- Layout is Bevy's own parley pipeline (`TextPipeline`), so world text has the same fonts and OS
  fallback as `<text>` (`docs/fonts.md`).
- Glyphs are rasterised once by Bevy's font atlas, turned into a signed distance field on the CPU,
  and kept in one engine-made SDF atlas. Each label is one `Mesh3d`, a quad per glyph. Labels
  share a few material instances (one per lit and outline style), drawn alpha-to-coverage with depth
  written: terrain hides them, they need no sorting, they stay crisp near and far, and the outline
  is a threshold in the shader.
- Rust users get the same with `WorldText` and `WorldTextStyle` components, no TSX needed.
- A bench of 1,000 labels: build time and frame cost.
- Stage 2, later and separate: text along a path (`path` prop), glyph by glyph.

No new dependency. A single-channel SDF rounds sharp corners only when a label is magnified far
past its raster size; MSDF (`fdsm`) is not needed for this change.

Not in this change: labels that face the camera (billboards), labels of a constant screen size,
`<span>` rich text in world text, text decorations (underline, background, shadow), multi-page
atlases (more than about 4,600 distinct glyphs), colour emoji in colour.

## Capabilities

### New Capabilities

- `world-text`: text drawn in the 3D world under a `Camera3d`, from TSX or Rust: layout, the SDF
  atlas, the material, occlusion, outline, lit or unlit, limits and errors. Stage 2 adds text on a
  path.

### Modified Capabilities

- `jsx-runtime`: `<worldText>` joins the intrinsics, a component of its entity.

## Impact

- Code: `crates/bevy-ts-material/src/world_text/` (components, layout, SDF atlas, mesh, material,
  `world_text.wesl`), `crates/bevy-ts-material/Cargo.toml` (Bevy's `bevy_text` feature, already in
  the engine through `bevy_ui`), `crates/bevy-ts-tsx/src/ui.rs` (the `worldText` kind),
  `resolve.rs`, `reconcile.rs`, `packages/bevy-ts/runtime/jsx-runtime.js`,
  `packages/bevy-ts/jsx-runtime.d.ts`.
- Tests: `crates/bevy-ts-material/tests/it/` (SDF, layout, render), `crates/bevy-ts-tsx` unit
  tests, `crates/bevy-ts-cli/tests/it/world_text.rs`, `packages/bevy-ts/test/tsx/elements.tsx`.
- Bench: `bench/world-text/`, 1,000 labels, out of the gate's build.
- Docs: new `docs/world-text.md`; `docs/ui.md` and `docs/fonts.md` link to it.
- Performance: a `PostUpdate` system that does work only when a label changes; one R8 atlas
  texture, 1 MB at first, at most 16 MB (4096²). The bench measures the frame cost. On the
  ask-first list: the lead decides whether it needs the operator's go.
- A new pattern: the first engine-made texture atlas and built-in material in bevy-ts (not a
  script `material()` kind). It follows `MaterialAlpha`'s shape (`alpha.rs`, `fade.wesl`).
- Protocol: unchanged.
