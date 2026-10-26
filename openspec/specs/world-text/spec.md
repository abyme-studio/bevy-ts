# world-text Specification

## Purpose
Text in the 3D world: `<worldText>` labels laid out by Bevy's text pipeline, drawn under any `Camera3d` from one shared distance-field atlas with one material per style, occluded and lit like other geometry, with an optional outline; the atlas's limit and failed fonts named; unchanged labels cost no layout.

## Requirements

### Requirement: worldText draws text in the 3D world
`<worldText>` SHALL be a component intrinsic of its `<entity>`. Its children, strings and bindings, SHALL be its text, joined as `<text>` joins them; a `<span>` or any other element inside it SHALL fail the load naming its location. The text SHALL lie in the entity's local XY plane, reading along +X with +Y up and facing +Z, placed by the entity's `Transform` and its parents'. It SHALL draw under any `Camera3d`, the editor's included, and SHALL be culled from behind. A frustum SHALL cull a label whose bounds are outside it.

#### Scenario: A label under a Camera3d
- **WHEN** a scene has a `Camera3d` 5 m from an entity holding `<worldText size={1} color="#fff">HELLO</worldText>`, facing it, rendered to an offscreen image
- **THEN** the image holds white pixels inside the projected bounds of the label and none outside them

#### Scenario: Seen from behind
- **WHEN** the camera moves behind the same label
- **THEN** no pixel of the label is drawn

### Requirement: worldText lays out with the fonts of `<text>`
World text SHALL be laid out by Bevy's `TextPipeline`, with the fonts, weights and OS fallback `<text>` uses (`text-fonts`). `font`, `weight`, `italic`, `justify` and `linebreak` SHALL take the values `<text>` takes and be parsed by the same code; `lineHeight` SHALL take a multiple of `size` only. A label whose font is still loading SHALL draw once it loads.

#### Scenario: Fallback in the world
- **WHEN** a label holds `Paris 中文 →`
- **THEN** every cluster of its layout has a glyph id other than 0 on a machine where the `text-fonts` test passes

#### Scenario: A project font
- **WHEN** a label sets `font="fonts/Inter.ttf"` and the font loads a frame later
- **THEN** the label draws with Inter's glyphs once it is loaded, with no error

### Requirement: worldText props are in world units and bindable
`size` SHALL be the em size in world units (default 1). `color` SHALL be the fill colour, in any colour form `<text>` takes (default white). `letterSpacing` SHALL be a fraction of `size` (default 0). `width` SHALL be the wrap width in world units (default none: no wrap). `anchor` SHALL be the point of the layout box at the entity's origin: `"center"` (default), `"top"`, `"bottom"`, `"left"`, `"right"`, `"top-left"`, `"top-right"`, `"bottom-left"`, `"bottom-right"`, or `[x, y]` from -0.5 to 0.5 with `[-0.5, -0.5]` the bottom left. `outline` SHALL be `{ width, color }`, `width` a fraction of `size` from 0 to 0.125. `lit` SHALL make the label take the scene's light (default false: unlit). Every prop and the text SHALL be bindable; a changed binding SHALL write only what it changes, and a changed text or layout prop SHALL rewrite the label's own mesh in place, with no new mesh asset.

#### Scenario: Size and anchor in world units
- **WHEN** a label `<worldText size={2} anchor="bottom-left">AB</worldText>` is laid out
- **THEN** its layout box (`ComputedWorldText.rect`) starts at x 0 and y 0 in the entity's space, its height is within 10% of the font's ascent plus descent at 2 world units, and its glyph quads lie in the box but for the distance field's spread

#### Scenario: Bound text rewrites the same mesh
- **WHEN** a label's text is `{() => "HP " + hp()}` and `hp` changes from 10 to 9
- **THEN** the label's `Mesh3d` keeps its handle, the mesh's quad count matches the new text, and no other label's mesh changes

#### Scenario: Wrapping
- **WHEN** a label with `width={4}` and `size={1}` holds a sentence wider than 4 world units
- **THEN** its layout has more than one line and no glyph's quad lies past 4 world units from its left edge

### Requirement: worldText refuses what it cannot draw
At load, these SHALL fail with an error naming `<worldText>`, the prop and the element's file and line, and SHALL NOT panic: a prop `<worldText>` does not take (`<text>`'s `style`, `textShadow`, `underline`, `strikethrough`, `textBackground` among them); a `size` or `width` that is not positive and finite; an `outline.width` outside 0 to 0.125; an `anchor` that is not one of the names or a pair within -0.5 to 0.5; a value of the wrong type. A `<worldText>` on an entity that also has `<mesh>` or `<material>` SHALL fail naming both. A binding that yields a bad value SHALL log the same error and keep the label's last good look.

#### Scenario: An outline too wide
- **WHEN** a scene writes `<worldText outline={{ width: 0.3, color: "#000" }}>A</worldText>`
- **THEN** the load fails naming `<worldText>`, `outline.width`, the range 0 to 0.125 and the element's file and line

#### Scenario: worldText and mesh on one entity
- **WHEN** an entity holds both `<mesh shape="cuboid" />` and `<worldText>A</worldText>`
- **THEN** the load fails naming both elements and the entity's file and line

### Requirement: World text writes depth and is hidden by what stands in front
World text SHALL draw in the opaque passes, alpha-to-coverage, and SHALL write depth, so geometry in front of a label hides it and a label in front of geometry hides that, with no sorting. Under a depth prepass, the prepass SHALL drop the same pixels the main pass drops, so the space between glyphs hides nothing. A label SHALL cast no shadow.

#### Scenario: Behind a wall
- **WHEN** a red cube stands between the camera and the left half of a white label
- **THEN** the image holds white pixels only right of the cube's projected edge, and red pixels where the left half would be

#### Scenario: Under a depth prepass
- **WHEN** the camera has `DepthPrepass` and a label stands in front of a red cube
- **THEN** red pixels show between the label's glyphs

### Requirement: World text stays crisp near and far
Glyphs SHALL be drawn from a signed distance field, antialiased by screen-space derivatives, so a label's edge is sharp when magnified and does not shimmer when small. The distance field SHALL be made from Bevy's own glyph raster, with no font crate other than Bevy's.

#### Scenario: Close up
- **WHEN** a glyph covers 400 pixels of height on screen
- **THEN** across its straight stems the fill goes from background to full colour within 2 pixels

#### Scenario: Far away
- **WHEN** a label's em size projects to 6 pixels and the camera moves by a tenth of a pixel per frame for 10 frames
- **THEN** the label's total drawn coverage changes by less than 10% between any two frames

### Requirement: Outline and lighting come from the material
`outline` SHALL draw a band of `outline.color` of `outline.width` times `size` around every glyph, from the same distance field, with no second mesh. An unlit label SHALL draw its colour unchanged by lights. A lit label SHALL be shaded as a `StandardMaterial` of its colour, facing +Z.

#### Scenario: An outlined label
- **WHEN** a white label has `outline={{ width: 0.1, color: "#000" }}` on a red background
- **THEN** black pixels lie between every white region and the red, and none lie more than `0.1 * size` (projected, plus 2 pixels) outside the glyphs

#### Scenario: Lit against unlit in the dark
- **WHEN** two white labels stand in a scene with no light and no ambient light, one `lit`
- **THEN** the unlit one draws white and the lit one draws darker than 10% grey

### Requirement: Labels share their atlas and materials
All world text SHALL share one SDF atlas texture. Each distinct glyph (font, glyph id, weight, style) SHALL be rasterised and turned into a distance field once, however many labels use it. Labels with equal `lit` and `outline` SHALL share one material instance; fill colour SHALL be per vertex. The atlas SHALL grow as glyphs are added, up to 4096 by 4096 texels, without rebuilding any label's mesh. A glyph that does not fit SHALL log one error naming the atlas limit, and SHALL draw nothing while the rest of the text, and every other label, draws; glyphs already in the atlas stay.

#### Scenario: A thousand labels
- **WHEN** 1,000 labels with distinct names, two colours and one outline style mount
- **THEN** there is one atlas image and one world-text material instance, and each distinct glyph was rasterised once

#### Scenario: The atlas is full
- **WHEN** a label asks for a glyph that does not fit in an atlas at its limit
- **THEN** one error names the limit, the label draws its other glyphs, and asking for that glyph again logs nothing more

#### Scenario: The atlas grows
- **WHEN** a label adds enough new glyphs to grow the atlas
- **THEN** every existing label keeps its mesh unchanged and still draws its glyphs

### Requirement: World text from Rust
`WorldText` (the text) and `WorldTextStyle` (`size`, `letter_spacing`, `width`, `anchor`, `outline`, `lit`) SHALL be reflected Rust components that, with Bevy's `TextFont` (its `font_size` unused), `TextColor`, `TextLayout` and `LineHeight`, draw a label with no TSX. `<worldText>` SHALL write exactly these components. The label's `Mesh3d` and material SHALL be owned by world text: a script or a system that writes them is not supported.

#### Scenario: A label spawned in Rust
- **WHEN** a Rust system spawns `(WorldText::new("Paris"), WorldTextStyle::default(), Transform::default())`
- **THEN** the next frame the entity has a `Mesh3d` with 5 quads and the world-text material

### Requirement: Unchanged labels cost no layout
World text SHALL lay out and mesh a label only when its text, font, layout props or style changed, or a font it waits on loaded. A frame in which no label changed SHALL lay out none.

#### Scenario: A still frame
- **WHEN** 1,000 labels have mounted and drawn, and a frame passes with no change
- **THEN** that frame lays out 0 labels

### Requirement: No text along a path yet
`<worldText>` SHALL NOT take `path` or `normal` yet: either SHALL fail the load as a prop `<worldText>` does not take, naming the prop and the element's location. Text along a path is the change `world-text-stage-2`.

#### Scenario: A path prop
- **WHEN** a label sets `path={[[0, 0, 0], [1, 0, 0]]}`
- **THEN** the load fails naming `<worldText>`, `path` and the element's file and line
