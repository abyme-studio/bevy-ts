## Why

2D renders in every bevy-ts engine today (`bevy_sprite_render` comes in through `bevy_ui_render`), but a script has no TSX for it. There is no `Mesh2d` or `ColorMaterial`, no atlas layout, no `Text2d` element, and no example, test or doc that draws 2D. `<Camera2d />` and `<Sprite image>` should work through the reflect path but are untested. Audit row 12 (`bevy-coverage.md`) marks 2D missing. Research (`docs/research/script-2d.md`, 2fb92f7) mapped Bevy 0.20's 2D surface onto TS and the neighbours that own parts of it.

## What Changes

Five stages, each landable on its own, in this order:

1. **Meshes and cameras (after mesh-primitives).** New intrinsics `<mesh2d>` and `<material2d>`, Bevy's `Mesh2d` and `MeshMaterial2d<ColorMaterial>`. `<mesh2d>` takes mesh-primitives' 2D shapes (`circle`, `rectangle`, `ellipse`, `annulus`, `regularPolygon`, `rhombus`, `triangle2d`, `capsule2d`, `circularSector`, `circularSegment`, `convexPolygon`) with their props, and no `depth`. `<material2d>` takes `color`, `alpha`, `alphaMode` (`"opaque"`, `"blend"`, `{ mask: 0.5 }`, Bevy's `AlphaMode2d`), `texture` (an asset path) and `uvTransform`. `<mesh2d color>` is shorthand for `<material2d color>`. Tests prove `<Camera2d />` draws a 2D scene. A new example, `examples/shapes-2d`.
2. **Sprites (after reflect-enums-color).** `<Sprite>` takes every field in Bevy's names and shapes: `image` (an asset path), `color`, `custom_size`, `rect`, `flip_x`, `flip_y`, `image_mode`, `alpha_mode`. `<Anchor value>` sets the anchor. Sprite picking: `onPointer*` on a sprite entity fires from Bevy's sprite backend. The example gains sprites.
3. **Atlases and 2D text (after reflect-coverage stage 1 and bevy-ui-coverage stage 2).** `TextureAtlasLayout.fromGrid({ tileSize, columns, rows, padding, offset })` returns a value; Rust builds one layout per distinct value, cached like `<mesh>` assets, and writes its handle. `<Sprite texture_atlas={{ layout: TextureAtlasLayout.fromGrid(...), index: 3 }} />` draws a frame. UI's `<image atlas>` takes the same value. New intrinsic `<text2d>`: Bevy's `Text2d` with UI's `<text>` props (`size`, `color`, `font`, `justify`, ...), `<span>` children, bound children and props, plus `anchor` and `bounds`.
4. **Sprite-sheet animation (after reflect-coverage stage 3, which needs the operator's go).** Bevy's pattern: a TS system on a timer writes `sprite.texture_atlas.index`. script-2d adds the example, the test and the doc; the write path is reflect-coverage's.
5. **Tilemap chunks (last).** `<TilemapChunk>` and `<TilemapChunkTileData>` with Bevy's fields. `tileset` loads as an array texture with `array_layout` from the tile grid. A change to either re-inserts the component, as Bevy's `TilemapChunk` is immutable.

Across stages:

- **A coverage table** for the whole 2D category in `docs/2d.md`, each row naming its test or its owner.
- **Bevy's names.** Rust components keep their field names (`custom_size`, `texture_atlas`). Intrinsics use camelCase props as `<mesh>` does.
- **No `spawnNative` recipe** for any 2D type.
- **No protocol change expected.** If one shows, the next free minor at landing.
- **Out of scope, follow-ups:** a 2D editor camera in the studio (the editor camera is a `Camera3d`, so 2D is invisible in edit mode); `Wireframe2d`, custom `Material2d` (a script material with a 2D target), `SpriteMesh`; spawning sprites from a system (`Sprite.of()`, reflect-coverage stage 2); `Assets.add` (script-assets).

## Capabilities

### New Capabilities

- `script-2d`: 2D in TSX: `<mesh2d>`, `<material2d>`, `<text2d>`, sprites and their props, atlas layouts as values, `Camera2d`, sprite picking, sprite-sheet animation and tilemap chunks.

### Modified Capabilities

- `jsx-runtime`: "Entity, component, prefab and intrinsic elements" lists `<mesh2d>`, `<material2d>` and `<text2d>` among the intrinsics.

## Impact

- Nothing here needs the operator's go. No new dependency (`bevy_sprite`, `bevy_sprite_render`, `bevy_text` are built today), no new pattern (atlas layouts and 2D materials are cached in `IntrinsicAssets`, as meshes are), no per-frame work (assets build once per distinct value), no data, cost or production effect. Stage 4 depends on reflect-coverage stage 3, which needs its own go; script-2d does not ask for it.
- Code: `crates/bevy-ts-tsx/src/intrinsics.rs` (`Mesh2dProps`, `Material2dProps`, atlas layouts, caches), `resolve.rs` and `reconcile.rs` (new intrinsic kinds), `ui.rs` (`<text2d>` reuses text props; `<image atlas>` takes the layout value), the reflect writer (a `Handle<TextureAtlasLayout>` takes a `fromGrid` value; a `TilemapChunk` tileset loads as an array texture).
- Types: `packages/bevy-ts/jsx-runtime.d.ts` (new intrinsics), `packages/bevy-ts` exports `TextureAtlasLayout.fromGrid`; generated `bevy.d.ts` types for Sprite props come from reflect-enums-color.
- Docs: new `docs/2d.md` (coverage table, every element and prop), linked from `README.md`.
- Example: new `examples/shapes-2d`, growing each stage.
- Tests: new `crates/bevy-ts-tsx/tests/two_d.rs` (headless component and asset checks per stage), picking and camera tests, `packages/bevy-ts/test/tsx/elements.tsx` type cases.
