# 2D in TS: what exists, what neighbours own, what script-2d builds

Research for `script-2d` (audit row 12, `bevy-coverage.md`). Read 2026-10-23 against this
worktree at `70880c6`, the neighbours' branches, and Bevy 0.20.0 in the cargo registry.

Headline: 2D already renders in every bevy-ts engine. What is missing is a TSX surface:
`<mesh2d>` and `<material2d>` (new), a cached atlas-layout value (new, shared with UI), a
`<text2d>` intrinsic (new, reusing UI text props), and examples. Colour, enum, `Option` and
nested-handle props come from reflect-enums-color and reflect-coverage. Sprite-sheet
animation from a system waits on reflect-coverage stage 3.

## What exists today

- The engine compiles 2D. `bevy-ts-cli` enables `3d` and `bevy_ui_render`; `bevy_ui_render`
  pulls `bevy_sprite_render` (bevy_internal `Cargo.toml`), so `SpritePlugin`,
  `ColorMaterialPlugin`, `Text2d` and `TilemapChunkPlugin` are all in. `3d` brings `picking`,
  which includes `sprite_picking`.
- `packages/bevy-ts/bevy.d.ts` names every 2D type. `Camera2d`, `Mesh2d`, `Text2d`,
  `TextColor`, `TilemapChunk`, `TilemapChunkTileData` are reflect tier (bare). `Sprite` is
  layout tier with only `flip_x`/`flip_y` visible; `image`, `texture_atlas`, `color`,
  `custom_size`, `rect`, `image_mode`, `alpha_mode` are holes (`bevy.d.ts:792`). `Anchor` is a
  newtype over `Vec2`. `TextFont` shows only `font_smoothing`; `TextLayout` is a full column.
  No `ColorMaterial` or `MeshMaterial2d` type at all (assets are not components).
- TSX: `<mesh>` (cuboid, sphere, plane) and `<material>` (StandardMaterial or a script
  material) always insert `Mesh3d` / `MeshMaterial3d` (`crates/bevy-ts-tsx/src/intrinsics.rs`).
  Equal props share one handle through `IntrinsicAssets`.
- A Rust component element whose layout has heap holes (Sprite) or no layout (Camera2d) goes
  the reflect path: inserted from `ReflectDefault`, each prop written by reflection, a string
  in a top-level `Handle<T>` field loaded as an asset path (`resolve.rs:250`,
  `reflected.rs`). So `<Camera2d />` and `<Sprite image="hero.png" />` should work today
  (untested). `color` takes only Bevy's reflected shape (`{ Srgba: {...} }`), `custom_size`
  only `{ Some: [w, h] }`, and `texture_atlas` fails: its `layout` handle is nested.
- Reflect-path props bind (corrected by reflect-bindable: the runtime always bound them; tsc
  refused an enum's bound `value` until then). Layout-tier holes bind too (layout-bindable).
- No example, test or doc mentions 2D. The editor camera is a `Camera3d`
  (`bevy-ts-viewport/src/edit.rs:490`), and Bevy draws sprites and `Mesh2d` only through a
  `Camera2d`, so a 2D scene is invisible from the editor camera.

## Bevy 0.20's 2D surface (the coverage table draft)

| Bevy | Shape in 0.20 | TS today | Proposed TS | Owner |
|---|---|---|---|---|
| `Camera2d` | marker; requires `Camera`, orthographic `Projection` | `<Camera2d />` (untested) | same, plus `<Projection value={{ Orthographic: { scale } }} />` | reuse; reflect-enums-color (enum) |
| `OrthographicProjection` `scale`, `scaling_mode`, `viewport_origin` | enum with data | missing | as above | reflect-enums-color |
| `Mesh2d` + 2D primitives (`Circle`, `Rectangle`, `Ellipse`, `Annulus`, `RegularPolygon`, `Rhombus`, `Triangle2d`, `Capsule2d`, `CircularSector`, `CircularSegment`, `ConvexPolygon`) | `Mesh2d(Handle<Mesh>)` | missing | `<mesh2d shape="circle" radius={40} />` | script-2d, on mesh-primitives' builders |
| `MeshMaterial2d<ColorMaterial>`: `color`, `alpha_mode` (`AlphaMode2d`: Opaque, Mask, Blend), `texture`, `uv_transform` | asset | missing | `<material2d color alpha alphaMode texture />`; `<mesh2d color>` shorthand | script-2d |
| `Sprite.image` | `Handle<Image>` | path rule (untested) | `<Sprite image="hero.png" />` | reuse |
| `Sprite.color`, `custom_size`, `rect`, `flip_x/y`, `alpha_mode` | `Color`, `Option<Vec2>`, `Option<Rect>`, bools, enum | flips only | props in TSX's colour/enum/Option forms | reflect-enums-color |
| `Sprite.image_mode`: `Auto`, `Scale(SpriteScalingMode)`, `Sliced(TextureSlicer)`, `Tiled {..}` | enum with data | missing | `imageMode={{ Sliced: { border: ... } }}`, helpers shared with UI's `sliced()`/`tiled()` | reflect-enums-color; helpers bevy-ui-coverage |
| `Anchor` (component, `TOP_LEFT`...) | newtype `Vec2` | `"0"` field, unwritable in JSX | `<Anchor value={[-0.5, 0.5]} />` | `value` rule (script-gltf / bevy-ui-coverage) |
| `TextureAtlas { layout, index }` | nested handle + usize | fails | `textureAtlas={{ layout: TextureAtlasLayout.fromGrid({...}), index: 3 }}` | script-2d (layout value); reflect-coverage stage 1 (nested handles) |
| `TextureAtlasLayout::from_grid(tile_size, columns, rows, padding?, offset?)` | asset, no loader | missing | `TextureAtlasLayout.fromGrid({ tileSize, columns, rows, padding?, offset? })`, a value Rust turns into a cached handle | script-2d |
| Sprite-sheet animation (system advances `texture_atlas.index` on a `Timer`) | system writes a hole | missing | Bevy's pattern in a TS system: `sprite.texture_atlas.index = ...` | reflect-coverage stage 3 (operator go pending) |
| `Text2d` + `TextFont`, `TextColor`, `TextLayout`, `TextBounds`, `TextSpan` children, `Text2dShadow` | reflect tier / holes | missing | `<text2d size color justify anchor>` with string and bound children, as `<text>` | script-2d, props from bevy-ui-coverage stage 2 |
| `TilemapChunk { chunk_size, tile_display_size, tileset, alpha_mode }` + `TilemapChunkTileData(Vec<Option<TileData>>)` | immutable component; array texture | reflect only | later stage, see Q3 | script-2d (later) |
| Spawn any of the above from a system | — | `commands.spawn(<element/>)` for intrinsics | `Sprite.of({...})` | reflect-coverage stage 2 |
| Cursor to world, world to screen | — | missing / landing | `Cameras.worldToViewport` works for any camera | world-to-screen, camera-rays |
| Sprite and `Mesh2d` picking | backends compiled | untested | `onPointer*` on a sprite: a test | script-2d (test only) |
| `Wireframe2d`, `Mesh2dWireframe`, custom `Material2d`, `SpriteMesh` | | reflect markers | out of scope | — |

## Neighbours: build, reuse, wait

| Change | State | What 2D takes from it | script-2d |
|---|---|---|---|
| reflect-enums-color | proposed, building | one colour parser (`bevy_ts::color`); enum, `Option`, nested-struct props on layout-tier holes; `Sprite.color` named in its list | wait for it; every Sprite prop rides on it |
| reflect-coverage | proposed (3 stages) | stage 1: nested `Handle<T>` as path (atlas `layout`); stage 2: `Sprite.of()` in commands; stage 3: hole reads and writes in systems (go needed) | wait; sprite-sheet animation needs stage 3 |
| mesh-primitives | building | Rust builders for all 11 2D shapes and their checks; it scopes out `Mesh2d` | reuse its shape parser; insert `Mesh2d` instead of `Mesh3d` |
| material-alpha | building | `alpha`, `alphaMode` names and parser on `<material>` | reuse the names; `AlphaMode2d` has only `opaque`, `mask`, `blend` |
| bevy-ui-coverage | building (8 commits) | stage 2 text props (`font`, `weight`, `justify`, spans); stage 7 `<image atlas={{ layout, index }}>` with `layout` undefined; `sliced()`/`tiled()` helpers; `value` for newtypes | reuse text props and helpers; script-2d owns the atlas-layout value and UI takes it |
| script-gltf | building | the same `value` rule for one-field tuple structs | reuse whichever lands first |
| world-to-screen | building | `Cameras` is camera-agnostic | reuse |
| game-camera-edit | building | shows game cameras (incl. `Camera2d`) in edit mode | reuse; a 2D editor camera is a follow-up |
| text-input | building | nothing 2D needs | — |
| script-assets | no change, no team, no branch | `Assets.add` of meshes, images, atlas layouts | do not wait (Q2) |

## Questions, with recommendations

1. `<material2d>` or `<material>` on a 2D mesh? Recommend `<mesh2d>` + `<material2d>`. Bevy
   keeps `Mesh2d`/`MeshMaterial2d` apart from `Mesh3d`/`MeshMaterial3d`; whether a mesh is 2D
   is a property of the entity, not the shape (a `Circle` is also a valid `Mesh3d`). A shared
   `<material>` would take StandardMaterial props (`metallic`, `roughness`, `emissive`) that
   mean nothing on `ColorMaterial`, and `tsc` could not tell which applies. Pairs mirror
   today's `<mesh>`/`<material>`, and leave room for `<material2d kind>` once script materials
   gain a 2D target. Shapes and checks come from mesh-primitives, restricted to its 2D members
   without `depth`.
2. Atlas layouts without script-assets? Recommend script-2d own a minimal piece, in the
   intrinsics pattern: `TextureAtlasLayout.fromGrid({...})` in TS returns a plain value
   (`{ fromGrid: {...} }`); Rust builds the layout once per distinct value into
   `IntrinsicAssets` and writes the handle. No new pattern, no writable param, nothing
   ask-first. bevy-ui-coverage's `<image atlas>` takes the same value, so one owner. A later
   script-assets handle fits the same slot. Waiting would block on an unproposed change that
   needs `writable-native-params` (ask-first).
3. TilemapChunk now? Recommend a last stage of script-2d, not a first. A static map works on
   reflect-enums-color plus one gap: `tileset` must load as an array texture (Bevy's example
   loads with `ImageLoaderSettings { array_layout }`), which the path rule cannot express.
   Editing tiles from a system needs reflect-coverage stage 3, and `TilemapChunk` is
   immutable, so TSX must re-insert it on change rather than write a field. Ship sprites,
   meshes and text first.
4. Sprite-sheet animation. Recommend Bevy's own pattern (a TS system on a timer writes
   `sprite.texture_atlas.index`) via reflect-coverage stage 3, not a script-2d workaround.
   Until it lands, script-2d ships static atlas frames and a test. The alternative, letting
   reflect-path props bind to signals, also touches the per-frame path and belongs to
   reflect-enums-color's writer, not here.

## Should this become a change

Yes, staged: (1) `<mesh2d>`, `<material2d>`, `<Camera2d>` example and docs after
mesh-primitives lands; (2) sprites with images, colour, sizing and slicing after
reflect-enums-color; (3) atlas layouts and `<text2d>`; (4) sprite-sheet animation after
reflect-coverage stage 3; (5) tilemap chunks. An `examples/` 2D project and headless tests
(component and asset checks, as mesh-primitives' `primitives.rs`) in each stage.

Not in scope, worth a later change: a 2D editor camera in the studio.

## Sources and confidence

- bevy-ts: `crates/bevy-ts-tsx/src/{intrinsics,resolve,reflected}.rs`,
  `packages/bevy-ts/{bevy,jsx-runtime}.d.ts`, `crates/bevy-ts-viewport/src/edit.rs`,
  `crates/bevy-ts-cli/Cargo.toml`.
- Neighbours: proposals, designs and tasks on branches `reflect-enums-color` (both changes),
  `mesh-primitives`, `material-alpha`, `bevy-ui-coverage`, `script-gltf`, `world-to-screen`,
  `game-camera-edit`; `abyme call agents list` for live teams (no script-assets team).
- Bevy 0.20.0: `bevy`/`bevy_internal` `Cargo.toml` features; `bevy_sprite` `sprite.rs`,
  `text2d.rs`, `texture_slice`; `bevy_sprite_render` `mesh2d/color_material.rs`,
  `tilemap_chunk`; `bevy_image` `texture_atlas.rs`, `image_loader.rs`; `bevy_text` `text.rs`.

High confidence on what exists and on the Bevy shapes (read in source). Medium on "should work
today" for `<Camera2d />` and `<Sprite image>` and on sprite picking: read from code, not run.
