## ADDED Requirements

### Requirement: `<mesh2d>` builds a `Mesh2d` from Bevy's 2D primitives
`<mesh2d shape>` SHALL insert Bevy's `Mesh2d` on its entity, built from one of the 2D shapes `<mesh>` takes (`circle`, `circularSector`, `circularSegment`, `ellipse`, `annulus`, `rectangle`, `rhombus`, `regularPolygon`, `triangle2d`, `capsule2d`, `convexPolygon`), with the same props, defaults and load checks as that shape on `<mesh>`. It SHALL NOT take `depth` or `segments`. The mesh SHALL lie in the XY plane facing +Z. `color` SHALL be a shorthand for `<material2d color>`. Elements with equal shape and props SHALL share one mesh asset. Bevy draws a `Mesh2d` only with a 2D material and a `Mesh3d` only with a 3D one, so `<mesh2d>` or `<material2d>` on one entity with `<mesh>` or `<material>` SHALL fail the load naming the 2D element's source location. `<mesh2d color>` with a `<material2d>` on one entity SHALL fail the load naming the `<material2d>`, as `<mesh color>` with `<material>` does.

#### Scenario: Every 2D shape builds a Mesh2d
- **WHEN** a headless app spawns one `<mesh2d>` of each 2D shape with its default props
- **THEN** each entity has a `Mesh2d` and no `Mesh3d`, and its mesh has vertices, one +Z normal per vertex and a triangle list

#### Scenario: A 3D shape is refused
- **WHEN** a scene writes `<mesh2d shape="cuboid" />` or `<mesh2d shape="circle" depth={1} />`
- **THEN** `tsc` reports it, and loading fails with an error naming the shape or prop and the element's file and line

#### Scenario: 2D and 3D do not mix on one entity
- **WHEN** an entity has `<mesh shape="circle" /><material2d />`, or `<mesh2d shape="circle" /><material />`, or `<mesh2d shape="circle" color="#f00" /><material2d />`
- **THEN** loading fails at that element's file and line, saying to pair `<mesh>` with `<material>` and `<mesh2d>` with `<material2d>`, or to give the colour once

### Requirement: `<material2d>` is a `ColorMaterial`
`<material2d>` SHALL insert `MeshMaterial2d<ColorMaterial>`. It SHALL take `color` (every colour form bevy-ts parses), `alpha` (0 to 1, applied with `Color::with_alpha`), `alphaMode` (`"opaque"`, `"blend"` or `{ mask: n }`, Bevy's `AlphaMode2d`), `texture` (an asset path, loaded as an `Image`) and `uvTransform` (`{ translation?, rotation?, scale? }`, built into an `Affine2`). Unset, `alphaMode` SHALL follow Bevy's `ColorMaterial::from(Color)` rule: `"blend"` when the final alpha is below 1, else `"opaque"`. An unknown prop SHALL fail the load naming it. Equal props SHALL share one material asset. Props SHALL NOT be bindable, as on `<material>`.

#### Scenario: A see-through textured material
- **WHEN** `<entity><mesh2d shape="rectangle" size={[64, 32]} /><material2d color="#f80" alpha={0.5} texture="tiles/brick.png" /></entity>` mounts
- **THEN** its `ColorMaterial` has colour `#f80` at alpha 0.5, `AlphaMode2d::Blend` and a texture handle for `tiles/brick.png`

#### Scenario: Equal materials share an asset
- **WHEN** two entities have `<material2d color="#0af" />`
- **THEN** both `MeshMaterial2d` handles are the same asset

### Requirement: A `Camera2d` draws 2D
`<Camera2d />` on an `<entity>` SHALL spawn Bevy's `Camera2d` with its required `Camera` and orthographic `Projection`. `<Projection value={{ Orthographic: { ... } }} />` SHALL set its scale, scaling mode and viewport origin, in reflect-enums-color's enum form.

#### Scenario: A 2D scene renders through a Camera2d
- **WHEN** a headless app with rendering to an image target mounts a `Camera2d` and a red `<mesh2d shape="rectangle">` filling the view
- **THEN** the image's centre pixel is red

#### Scenario: A zoomed-out camera
- **WHEN** a scene writes `<Camera2d /><Projection value={{ Orthographic: { scale: 2 } }} />`
- **THEN** the camera's `Projection` is orthographic with scale 2

### Requirement: `<Sprite>` takes every field in Bevy's shape
`<Sprite>` SHALL take Bevy 0.20's `Sprite` fields by their Rust names: `image` (an asset path, loaded as an `Image`), `color`, `custom_size` (`[w, h]` or `null`), `rect` (`{ min, max }` or `null`), `flip_x`, `flip_y`, `image_mode` (`"Auto"`, `{ Scale: ... }`, `{ Sliced: ... }`, `{ Tiled: ... }`) and `alpha_mode` (`"Opaque"`, `"Blend"`, `{ Mask: n }`). Values SHALL use reflect-enums-color's forms for colours, enums and `Option`. The `sliced()` and `tiled()` helpers UI's `<image mode>` uses SHALL also be valid `image_mode` values. `<Anchor value={[x, y]} />` SHALL set the sprite's anchor. Generated `bevy.d.ts` types SHALL type each of these props, so a wrong shape is a `tsc` error.

#### Scenario: An image sprite
- **WHEN** `<entity><Sprite image="hero.png" custom_size={[32, 48]} color="#fff8" flip_x /></entity>` mounts
- **THEN** its `Sprite` has a handle for `hero.png`, `custom_size` `Some((32, 48))`, the colour and `flip_x` true

#### Scenario: A nine-slice panel
- **WHEN** a sprite has `image_mode={sliced({ border: 8 })}`
- **THEN** its `image_mode` is `SpriteImageMode::Sliced` with an 8 px border on every side

### Requirement: Sprites and 2D meshes are pickable
`onPointerClick`, `onPointerPress`, `onPointerRelease`, `onPointerOver` and `onPointerOut` on an `<entity>` with a `Sprite` or a `Mesh2d` SHALL fire from Bevy's picking backends (`SpritePickingPlugin`; for a `Mesh2d`, Bevy's `MeshPickingPlugin`, which `meshPicking` in `bevy-ts.json` adds as for 3D meshes) when a pointer moves or presses over it through a `Camera2d`, with no extra setup in the script.

#### Scenario: Clicking a sprite
- **WHEN** a headless app with a `Camera2d` and a sprite with `onPointerClick` feeds Bevy a mouse press and release at the sprite's screen position
- **THEN** the handler runs once with the sprite's entity

### Requirement: Atlas layouts are values
`TextureAtlasLayout.fromGrid({ tileSize, columns, rows, padding?, offset? })` from `bevy-ts/bevy` SHALL return a plain asset value, and SHALL be the one form `<image atlas>` `layout` takes besides a path or an asset ref. Where a `Handle<TextureAtlasLayout>` is written from TSX (a `Sprite`'s `texture_atlas.layout`, UI's `<image atlas>` `layout`), Rust SHALL build Bevy's `TextureAtlasLayout::from_grid` once per distinct value, cache it with the other script asset values, and write its handle. `tileSize`, `columns` and `rows` SHALL be positive; otherwise the load SHALL fail naming the prop. `texture_atlas` SHALL take `{ layout, index }`; an `index` past the layout's last tile SHALL fail the load naming the index and the tile count.

#### Scenario: One frame of a sprite sheet
- **WHEN** two sprites mount with `texture_atlas={{ layout: TextureAtlasLayout.fromGrid({ tileSize: [16, 16], columns: 8, rows: 4 }), index: 3 }}` and `index: 5`
- **THEN** both `Sprite.texture_atlas` values hold the same layout handle, a 32-tile grid layout, with indexes 3 and 5

#### Scenario: UI shares the layout
- **WHEN** an `<image atlas={{ layout, index: 1 }}>` uses the same `fromGrid` value as a sprite
- **THEN** both hold the same layout handle

### Requirement: `<text2d>` is world-space text
`<text2d>` SHALL spawn Bevy's `Text2d` on its entity, with the props `<text>` takes (`size`, `color`, `font`, `weight`, `italic`, `justify`, `linebreak`, `lineHeight`, `letterSpacing`, `textBackground`, `underline`, `strikethrough`), and `<span>` children, string children and bound children as `<text>` has them. It SHALL also take `textShadow` (`<text>`'s `{ x, y, color }`, writing `Text2dShadow` with y up), `anchor` (`<worldText>`'s names or `[x, y]`, Bevy's `Anchor`) and `bounds` (`[w, h]`, each a number or `null`, `TextBounds`). Props and children SHALL bind as on `<text>`, writing only what changed.

#### Scenario: A bound score above a sprite
- **WHEN** `<entity><Transform translation={[0, 40, 0]} /><text2d size={24} color="#fff" anchor={[0, -0.5]}>{() => `${score()}`}</text2d></entity>` mounts and `score` changes from 1 to 2
- **THEN** the entity has `Text2d` "1" then "2", `TextFont` size 24, `TextColor` white and `Anchor` `(0, -0.5)`, and no other component is written on the change

### Requirement: Sprite-sheet animation runs from a TS system
A TS system SHALL animate a sprite sheet the way Bevy's `sprite_sheet` example does: it reads a timer and writes `sprite.texture_atlas.index` on a queried `Sprite`. This SHALL work once TS systems can read and write a `Sprite`'s reflected fields (reflect-coverage stage 3); script-2d SHALL NOT add another path.

#### Scenario: Frames advance on a timer
- **WHEN** a system advances `texture_atlas.index` from 0 to 7 every 0.1 s and the app runs 0.35 s of fixed time steps
- **THEN** the sprite's index is 3

### Requirement: Tilemap chunks in TSX
`<TilemapChunk>` SHALL take Bevy's fields (`chunk_size`, `tile_display_size`, `tileset`, `alpha_mode`) and `<TilemapChunkTileData value>` an array of `{ tileset_index, color?, visible?, orientation? }` or `null` per tile. `tileset` SHALL be an array texture with one layer per tile: `Image.load(path, { array_layout })`, Bevy's `ImageArrayLayout`, which `Image.load` SHALL take beside `sampler` and `is_srgb`, or an image ref; a bare path or a load with no `array_layout` SHALL fail the load naming `array_layout`. A tile array whose length is not `chunk_size.x * chunk_size.y` SHALL fail the load naming both. Since Bevy's `TilemapChunk` is immutable, a changed prop SHALL insert the component again, so its hook rebuilds the chunk; the tile data SHALL be on the entity before the chunk whatever the source order.

#### Scenario: A static map
- **WHEN** a scene mounts a 4x4 `TilemapChunk` with a 16-entry tile array and `tileset={Image.load("tiles.png", { array_layout: { RowCount: { rows: 4 } } })}`
- **THEN** the entity has `TilemapChunk`, `TilemapChunkTileData` with 16 entries, the chunk's mesh and material, and the tileset image is loaded with one array layer per tile

#### Scenario: A changed chunk is inserted again
- **WHEN** a bound `tile_display_size` changes from 16 to 32
- **THEN** the entity's `TilemapChunk` has the new size and its other fields, and a new mesh

#### Scenario: A wrong tile count or a flat tileset
- **WHEN** a 4x4 chunk is given 15 tiles, or a `tileset` path with no `array_layout`
- **THEN** loading fails naming 15 and 16, or `array_layout`

### Requirement: The 2D coverage table
`docs/2d.md` SHALL list every part of Bevy 0.20's 2D surface (cameras, `Mesh2d` and its shapes, `ColorMaterial`, every `Sprite` field, `Anchor`, atlases, `Text2d` and its components, sprite-sheet animation, tilemap chunks, picking, `Wireframe2d`, custom `Material2d`, `SpriteMesh`) with its TS form and the test that proves it, or the change that owns it, or "out of scope" with the reason.

#### Scenario: Every row is backed
- **WHEN** a row's TS form is marked working
- **THEN** it names a test that exists in the repo
