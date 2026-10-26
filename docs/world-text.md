# World text

`<worldText>` draws text in the 3D world, under any `Camera3d`: city names on a map, a unit's name
over its head, a sign. `<text>` is screen UI ([ui.md](ui.md)). A label is a component of its
`<entity>`, as `<mesh>` is; its children, strings, numbers and bindings, are its text.

```tsx
<entity>
  <Transform translation={[0, 2, 0]} />
  <worldText size={0.5} outline={{ width: 0.1, color: "#000" }}>{() => name()}</worldText>
</entity>
```

The text lies in the entity's local XY plane, reading along +X with +Y up and facing +Z, placed by
the entity's `Transform` and its parents'. It is unlit, culled from behind, casts no shadow, and
does not turn to the camera. Geometry in front of a label hides it and a label hides what is behind
it, with no sorting. Every prop and the text can be bound; a changed binding writes only the
component behind it, and a changed text or layout prop rewrites the label's own mesh in place.

## Props

| Prop | Takes | Default |
|---|---|---|
| `size` | the em size in world units, positive | 1 |
| `color` | the fill, any colour `<text>` takes | white |
| `font`, `weight`, `italic` | as `<text>`'s ([fonts.md](fonts.md)) | Bevy's default font and the OS's fallback |
| `justify`, `linebreak` | as `<text>`'s | `left`, `word` |
| `lineHeight` | a multiple of `size` (a number; `px(n)` is refused) | 1.2 |
| `letterSpacing` | extra space between letters, a fraction of `size` | 0 |
| `width` | the wrap width in world units, positive; ignored with `linebreak="none"` | no wrap |
| `anchor` | the point of the layout box at the entity's origin: `"center"`, `"top"`, `"bottom"`, `"left"`, `"right"`, `"top-left"`, `"top-right"`, `"bottom-left"`, `"bottom-right"`, or `[x, y]` from -0.5 to 0.5, `[-0.5, -0.5]` the bottom left | `"center"` |
| `outline` | `{ width, color }`: a band of `color` around every glyph, `width` a fraction of `size` from 0 to 0.125 | none; `color` black |
| `lit` | `true`: shaded by the scene's lights as a `StandardMaterial` of its colour | `false`, unlit |

The layout box is the text's lines: as wide as the widest line (or `width`, when given), as tall as
its lines. `ComputedWorldText.rect` holds it in the entity's space once laid out.

## What fails the load

Each names `<worldText>`, the prop and the element's file and line:

- a prop `<worldText>` does not take: `<text>`'s `style`, `textShadow`, `underline`,
  `strikethrough`, `textBackground`, and `path` (text on a path is the change `world-text-stage-2`);
- `size` or `width` not a positive number; `outline.width` outside 0 to 0.125; an `anchor` that is
  not a name or a pair within -0.5 to 0.5; a value of the wrong type;
- a `<span>` or any element inside it;
- `<worldText>` beside `<mesh>` or `<material>` on one entity (named with the entity's line): a
  label draws its own mesh and material;
- `<worldText>` outside an `<entity>`.

A binding that yields a bad value logs the same error and the label keeps its last good look.

## From Rust

The components are the interface; `<worldText>` writes exactly these:

```rust
commands.spawn((
    WorldText::new("Paris"),
    WorldTextStyle { size: 0.5, outline: Some((0.1, Color::BLACK)), ..default() },
    TextColor(Color::WHITE),
    Transform::from_xyz(0.0, 2.0, 0.0),
));
```

`WorldText` requires `WorldTextStyle`, Bevy's `TextFont` (its `font_size` is not read: `size` is),
`TextColor`, `TextLayout`, `LineHeight`, `Transform`, `Visibility` and `NotShadowCaster`. In Rust,
`LineHeight::Px` is world units. The label's `Mesh3d`, `MeshMaterial3d<WorldTextMaterial>` and
`Aabb` are world text's: a script or system that writes them is not supported. Removing
`WorldText` removes them. `WorldTextPlugin` is in the engine; a Rust app adds it beside Bevy's
`DefaultPlugins`.

## How it works

The code is `crates/bevy-ts-material/src/world_text/`.

- **Layout** (`layout.rs`): a `PostUpdate` system lays out each label whose components changed, or
  whose font was still loading, with Bevy's `TextPipeline` at 48 px, so it gets the fonts and OS
  fallback `<text>` gets. Lengths are scaled by `size / 48`. A frame where no label changed lays
  out none (`LaidOut` counts them).
- **Distance field** (`sdf.rs`, `atlas.rs`): Bevy rasterises each glyph once into its font atlas;
  world text turns that raster into a signed distance field, 6 px past the edge, and keeps it in
  one shared `R8` atlas. Each distinct glyph is made once, however many labels use it.
- **Mesh** (`mesh.rs`): each label is one mesh, a quad per glyph, its fill colour per vertex, UVs in
  atlas texels.
- **Material** (`material.rs`, `world_text.wesl`): a `StandardMaterial` base, alpha-to-coverage,
  and an extension that reads the distance field and antialiases it by its screen-space derivative,
  so an edge stays sharp close up. It reads the field at four points in each pixel, so a far label,
  its strokes thinner than a pixel, does not shimmer as it moves. The outline is a second threshold on the same
  distance. Labels with the same `lit` and `outline` share one material instance, so a thousand
  labels of one style draw with one material and batch. The depth prepass drops the same pixels the
  main pass does.

## Limits

- **The atlas**: 4096 texels wide, 256 high at first (1 MB), doubled as glyphs fill it, up to 4096
  by 4096 (16 MB, about 4,600 glyphs). Growing rebuilds no mesh. Past 16 MB a new glyph logs one
  error naming the limit and draws nothing; the rest of its label, and every other label, still
  draws. Glyphs already made stay; nothing is evicted.
- **New glyphs**: a frame that makes a glyph uploads the whole atlas again (1 MB at first, up to
  16 MB). A game that keeps meeting new glyphs, say a stream of CJK names, pays that on each such
  frame; text made of glyphs already seen pays nothing.
- **Script types**: `bevy-ts/bevy`'s `WorldTextStyle` has no `width` or `outline` (its generator
  skips optional fields); set them through `<worldText>`'s props.
- **Outline width**: at most 0.125 of `size`, the distance field's spread.
- **Corners**: a single-channel distance field rounds a glyph's sharp corners when a label is
  magnified far past 48 px an em on screen.
- **No MSAA**: alpha-to-coverage falls back to a hard edge at half coverage. Bevy's cameras default
  to MSAA 4.
- **Memory**: Bevy's font atlas keeps each glyph's 48 px raster too, as `<text>` at one size does.
- Not here: labels that face the camera or keep a constant screen size, `<span>` rich text in a
  label, underline, background and shadow, colour emoji in colour (they draw as one-colour shapes),
  text along a path (the change `world-text-stage-2`).

## The bench

`bench/world-text/`: 1,000 labels from Rust (distinct names, two colours, one outline) at
1280×720 offscreen; `run.sh` runs its arms. On an M5 Max under load, 1,000 still labels cost no
more than an empty scene beyond the noise (a still frame lays out none); 100 labels changed a
frame cost about 4 ms (40 µs a label); 1,000 labels mount in one frame. One material, one 1 MB
atlas of 46 glyphs. The numbers are in `bench/world-text/results/README.md`.
