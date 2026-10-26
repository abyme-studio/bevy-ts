## Context

Research: `docs/research/script-2d.md` (2fb92f7). The engine already builds `SpritePlugin`, `ColorMaterialPlugin`, `Text2d`, `TilemapChunkPlugin` and sprite picking. `<mesh>`/`<material>` always insert `Mesh3d`/`MeshMaterial3d` (`crates/bevy-ts-tsx/src/intrinsics.rs`), with equal props sharing a handle through `IntrinsicAssets`. A Rust component with heap holes (`Sprite`) or no layout (`Camera2d`) goes the reflect path (`resolve.rs:250`, `reflected.rs`): inserted from `ReflectDefault`, props written by reflection, a string in a top-level `Handle<T>` loaded as a path. Reflect-path props cannot be bound (`resolve.rs:214`).

Neighbours own parts of 2D:

| Change | Gives script-2d | Stage that waits |
|---|---|---|
| mesh-primitives (building) | `MeshShape`, its 2D shapes, checks and builders | 1 |
| material-alpha (building) | `alpha`, `alphaMode` names and parser | 1 |
| reflect-enums-color (building) | `bevy_ts::color`; enum, `Option`, nested props on layout-tier holes (`Sprite.color` and the rest) | 2 |
| reflect-coverage stage 1 (proposed) | one reflect value shape, nested `Handle<T>` fields | 3 |
| bevy-ui-coverage stage 2 and 7 (building) | `<text>` props, `<span>`; `<image atlas>`, `sliced()`/`tiled()` | 2 (helpers), 3 |
| reflect-coverage stage 3 (proposed, needs the operator's go) | TS systems read and write reflected fields | 4 |
| world-to-screen (building) | `Cameras.worldToViewport` for any camera | none |

## Goals / Non-Goals

**Goals:** every part of Bevy 0.20's 2D surface has a TS form in Bevy's names, a test, and a row in `docs/2d.md`. Each stage lands on its own.

**Non-Goals:** a 2D editor camera; binding reflect-path props; custom `Material2d`, `Wireframe2d`, `SpriteMesh`; `Assets.add`; `Sprite.of()` from commands (reflect-coverage stage 2).

## Stages

| # | Stage | Depends on | Landable now |
|---|---|---|---|
| 1 | `<mesh2d>`, `<material2d>`, `Camera2d` tests, `examples/shapes-2d`, `docs/2d.md` | mesh-primitives, material-alpha | no: after mesh-primitives lands (material-alpha only for the shared alpha parser; stage 1 may copy names and switch when it lands) |
| 2 | `<Sprite>` props, `<Anchor value>`, `sliced()`/`tiled()` on `image_mode`, sprite and `Mesh2d` picking test | reflect-enums-color; bevy-ui-coverage stage 7 for the helpers | `image`, `flip_x/y`, picking and `Camera2d` tests: yes; the rest after reflect-enums-color |
| 3 | `TextureAtlasLayout.fromGrid`, `texture_atlas`, `<image atlas>` layout, `<text2d>` | reflect-coverage stage 1 (nested handle); bevy-ui-coverage stages 2 and 7 | no |
| 4 | sprite-sheet animation: example, test, doc | reflect-coverage stage 3 (operator's go) | no |
| 5 | `<TilemapChunk>`, `<TilemapChunkTileData>`, array-texture tileset | reflect-enums-color; stage 3's handle hook | no |

Each stage closes its rows in `docs/2d.md` and grows `examples/shapes-2d`.

## Decisions

### 1. `<mesh2d>` + `<material2d>`, not `<material>` on a 2D mesh

Bevy keeps `Mesh2d`/`MeshMaterial2d` apart from `Mesh3d`/`MeshMaterial3d`. Whether a mesh is 2D is a property of the entity, not the shape: a `Circle` is also a valid `Mesh3d`. A shared `<material>` would accept `metallic`, `roughness` and `emissive` on a `ColorMaterial`, and `tsc` could not tell which applies. The pair mirrors `<mesh>`/`<material>`, and leaves room for `<material2d kind>` once script materials gain a 2D target.

`Mesh2dProps` reuses mesh-primitives' `MeshShape` restricted to its 2D variants without `depth`/`segments`: one parser, one set of checks. The TS type is `Extract<MeshProps, { shape: Shape2d }>` minus `depth`/`segments`. The mesh cache is shared: one `Mesh` asset serves a `Mesh2d` and a `Mesh3d` of equal shape and props.

`Material2dProps`: `color`, `alpha`, `alphaMode`, `texture`, `uvTransform`. `alphaMode` takes `"opaque"`, `"blend"`, `{ mask: n }`, the subset of material-alpha's spellings that `AlphaMode2d` has. A separate `materials2d` map in `IntrinsicAssets`.

Alternative rejected: `<mesh dimension="2d">`. It is one element with two meanings and still needs a second material.

### 2. Atlas layouts are asset values

`TextureAtlasLayout.fromGrid({...})` from `bevy-ts/bevy` returns the asset value `{ $of: "bevy_image::texture_atlas::TextureAtlasLayout", props: { fromGrid: { tileSize, columns, rows, padding, offset } } }`. Bevy gives `TextureAtlasLayout` no reflected default, so `T.of` cannot build it; `bevy_ts::atlas_layout` hand-maps it instead, as `Image` and `Mesh` are, and builds `TextureAtlasLayout::from_grid`. Asset values are cached by their props, so every `Handle<TextureAtlasLayout>` slot given an equal grid shares one layout. The reflect writer reaches a sprite's `texture_atlas.layout` through its asset hook (reflect-coverage stage 1's nested handles); UI's `<image atlas>` calls the same hook. One owner, `atlas_layout.rs`, which also checks `texture_atlas.index` against the grid. UI's own grid cache and its `{ tile, ... }` form are gone (lead's ruling, 2026-10-27): `fromGrid` is the one form, named after Bevy's `tile_size`.

It needs no new pattern: hand-mapped asset values already turn JSON into cached handles, and an `assets.add` ref fits the same slot.

### 3. `<text2d>` is a text kind on UI's text props

`Text2d` takes the same `TextFont`, `TextColor`, `TextLayout`, `TextSpan` children as UI `Text`. `<text2d>` is a new kind beside `<text>` in `ui.rs`'s text path: same props parser, same bound-children writer, plus `anchor` and `bounds`; `textShadow` keeps `<text>`'s `{ x, y, color }` and writes `Text2dShadow`. That gives bound props and children on `<text2d>` without touching the reflect path. It inserts no `Node`. Like `<worldText>`, it is a component of its `<entity>`; its `<span>`s are child entities of that entity, as Bevy builds `Text2d` rich text.

### 4. Rust component props keep Rust names

`<Sprite custom_size>` and `texture_atlas`, not `customSize`. The reflect path writes fields by their Rust names, and reflect-enums-color's examples (`scaling_mode`) do the same. Intrinsic props stay camelCase, as `<mesh innerRadius>` does. The research table's `imageMode` and `textureAtlas` are corrected to `image_mode` and `texture_atlas`.

### 5. `sliced()`/`tiled()` on sprites

bevy-ui-coverage owns the helpers. `SpriteImageMode::Sliced(TextureSlicer)` and `Tiled { tile_x, tile_y, stretch_value }` have the same shape as UI's `NodeImageMode` variants, so the helpers work on sprites if they return Bevy's reflected shape. Stage 2 checks that; if UI's helpers return a UI-only form, stage 2 asks bevy-ui-coverage to return the reflected shape rather than adding a second set.

### 6. Picking and camera tests drive Bevy, not triggered events

The picking test feeds `PointerInput` to a headless app with a `Camera2d` rendering to an image target, so `SpritePickingPlugin` does the hit test. `tsx_reactive.rs`'s `click()` triggers `Pointer<Click>` directly and would not prove the backend. The sprite uses `custom_size` and `SpritePickingMode::BoundingBox` so no image needs to load. The camera test renders one frame to an `Image` target and reads its centre pixel; it runs where the GPU tests already run.

### 7. Tilemap tileset as an array texture

Bevy's example loads the tileset with `ImageLoaderSettings { array_layout: Some(ImageArrayLayout::RowCount { rows }) }`. The path rule cannot pass loader settings, so a `TilemapChunk`'s `tileset` string loads through `AssetServer::load_with_settings` with the row count from the image and `tile_display_size`. Because `TilemapChunk` is immutable, its insert hook builds the chunk mesh; TSX re-inserts it on a prop change instead of writing a field.

### 8. Protocol

No protocol change expected: intrinsic props do not cross the remote protocol. If one shows (an inspector answer naming a 2D intrinsic), take the next free minor at landing.

## Risks / Trade-offs

- [Stage 1 lands before material-alpha] → copy the three `AlphaMode2d` spellings into `Material2dProps`; switch to material-alpha's parser when it lands. Risk is one duplicated match for a short time.
- [Headless render-to-image may not run in CI] → the camera test is gated like other GPU tests; the asset and component checks in `two_d.rs` run everywhere.
- [2D is invisible in the editor] → named as a follow-up; `examples/shapes-2d` says to use play.
- [`texture_atlas` index out of range] → checked at load against the built layout's tile count, not left to Bevy, which would draw nothing.

## Open Questions

None blocking. The follow-up change for a 2D editor camera is not proposed here.
