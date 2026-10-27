# bitmap-fonts Specification

## Purpose
An XNA `SpriteFont` exported to JSON and PNG loads as a font, and `<bitmapText>` lays a string out in it as MonoGame does, one sprite per glyph on whole pixels.

## Requirements

### Requirement: XNA SpriteFont assets
A `SpriteFont` asset SHALL load from an XNA SpriteFont exported as JSON: `texture.export` (the PNG, beside the JSON, in the same source), `glyphs` (rects in the texture), `cropping`, `characterMap`, `verticalLineSpacing`, `horizontalSpacing`, `kerning` (per glyph: left bearing, width, right bearing) and `defaultCharacter`. Its loader SHALL be picked by asset type, so other JSON files are untouched. Lists of unequal length, or a missing texture, SHALL fail the load naming the field or the path.

#### Scenario: Stardew's small font
- **WHEN** `stardew://Fonts/SmallFont.json` loads as a `SpriteFont`
- **THEN** it has one glyph per `characterMap` entry, line spacing 33, spacing -1, default character `*`, and its texture is `stardew://Fonts/SmallFont.png`

#### Scenario: A broken font
- **WHEN** a font's `cropping` has one entry fewer than `glyphs`
- **THEN** the load fails naming `cropping` and both counts

### Requirement: Bitmap text lays out as MonoGame does
`<bitmapText font>` SHALL draw its text with one sprite per glyph, laid out as MonoGame's `SpriteFont`: at a line's first glyph x is its left bearing if positive, else 0; after it x grows by `horizontalSpacing` plus the next glyph's left bearing; each glyph draws at x plus its cropping offset, then x grows by its width and right bearing; `\n` starts a line `verticalLineSpacing` lower. A character outside the map SHALL draw `defaultCharacter`, or be skipped with one warning naming it when there is none. Positions SHALL be whole pixels times `scale`.

#### Scenario: Two glyphs
- **WHEN** a test font has `A` (bearings 1, 5, 1; cropping x 0) and `B` (bearings 0, 4, 1; cropping x 1) and spacing 1, and draws `AB`
- **THEN** `A` draws at x 1 and `B` at x 1 + 5 + 1 + 1 + 0 + 1 = 9

#### Scenario: A missing character
- **WHEN** Stardew's small font draws `é` and has no `é`
- **THEN** a `*` draws in its place

### Requirement: The bitmapText element
`<bitmapText>` SHALL take `font` (an asset path), `color` (tints every glyph), `scale` (default 1), `anchor` (as `<text2d>`'s, over the measured box, its point of the box rounded down to a whole pixel so glyphs stay on whole pixels) and its text as children: strings and bindings, joined. A changed binding SHALL re-lay the glyphs. The glyph sprites SHALL be the element's own children, hidden from the reconciler and from saved levels, and SHALL follow the element's `YSort` and `ScreenSize` by Bevy's propagation. An unknown prop SHALL fail naming it and the line.

#### Scenario: A bound label
- **WHEN** `<bitmapText font="fonts/tiny.json">{() => name()}</bitmapText>` mounts and `name` changes from `Al` to `Bea`
- **THEN** it has two glyph sprites, then three, and the entity's own components are unchanged
