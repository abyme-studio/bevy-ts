## 1. Stage 1: meshes, materials, Camera2d (after mesh-primitives lands)

- [x] 1.1 `crates/bevy-ts-tsx/src/intrinsics.rs`: `Mesh2dProps` on mesh-primitives' `MeshShape`, 2D variants only, refusing `depth`/`segments`; `insert_mesh2d`/`remove_mesh2d` sharing the mesh cache (design 1)
- [x] 1.2 `Material2dProps` (`color`, `alpha`, `alphaMode`, `texture`, `uvTransform`) with `deny_unknown_fields`; `ColorMaterial` built through `bevy_ts::color` (or today's hex parser until reflect-enums-color lands); `materials2d` cache in `IntrinsicAssets`; `<mesh2d color>` shorthand
- [x] 1.3 `resolve.rs`, `reconcile.rs`: the `mesh2d` and `material2d` kinds; `<mesh2d>` with `<mesh>`, or `<material2d>` with `<material>`, on one entity fails naming the source location
- [x] 1.4 `packages/bevy-ts/jsx-runtime.d.ts`: `Mesh2dProps`, `Material2dProps`, both in `IntrinsicElements`; `packages/bevy-ts/test/tsx/elements.tsx`: one valid element per 2D shape, `@ts-expect-error` for `shape="cuboid"`, `depth`, and `<material2d metallic>`; `bench/dx/bevy-ts/jsx-runtime.ts` copies `MeshProps` only for `<mesh>`, whose type is unchanged (the 2D shapes moved into `Shape2dMesh`, the same union), and its scenes use no 2D, so it gains nothing
- [x] 1.5 `crates/bevy-ts-cli/tests/it/two_d.rs` (TSX needs V8, so the cli tests own it): every 2D shape builds a `Mesh2d` (no `Mesh3d`) with +Z normals; a `<material2d>` with colour, alpha, texture builds the expected `ColorMaterial` and `AlphaMode2d`; equal props share handles; refusals name shape, prop and line
- [x] 1.6 `Camera2d` test: `<Camera2d />` plus a red full-view rectangle rendered to an image target gives a red centre pixel; `<Projection value={{ Orthographic: { scale: 2 } }} />` sets scale 2 (needs reflect-enums-color for the enum; skip that case until it lands)
- [x] 1.7 New `examples/shapes-2d` (`src/game.tsx`; no `example.json`, so `new` does not ship it): a `Camera2d` and every 2D shape in a grid, each its own colour
- [x] 1.8 New `docs/2d.md`: the coverage table (every row of design's surface, TS form, test or owner); link from `README.md`; a test reads the table and fails when a row marked working names a test that does not exist (the CI hole check)
- [x] 1.9 Gate: `cargo test -p bevy-ts-tsx`, the TS type tests, `protocol/bevy-ts.openrpc.json` unchanged

## 2. Stage 2: sprites (after reflect-enums-color lands)

- [x] 2.1 `two_d.rs`: `<Sprite image="hero.png" flip_x />` mounts with an image handle (works today; prove it before the rest)
- [x] 2.2 `two_d.rs`: `color`, `custom_size`, `rect`, `image_mode` (`"Auto"`, `{ Scale: ... }`, `{ Sliced: ... }`, `{ Tiled: ... }`), `alpha_mode` reach `Sprite` in Bevy's values; `<Anchor value={[-0.5, 0.5]} />` sets the anchor
- [x] 2.3 Check `sliced()`/`tiled()` from bevy-ui-coverage return Bevy's reflected shape and work as `image_mode`; if not, ask bevy-ui-coverage to change them (design 5) (they return a UI-only shape; lead@script-2d asked bevy-ui-coverage to return the reflected one)
- [x] 2.4 Check the generated `bevy.d.ts` types every `Sprite` prop; `elements.tsx` gains a wrong-shape `@ts-expect-error` (`custom_size={32}`)
- [x] 2.5 Picking test (design 6): a `Camera2d` with an image target, a sprite with `custom_size` and `SpritePickingMode::BoundingBox`, `PointerInput` press and release over it: `onPointerClick` runs once with its entity (sprite done: `events.rs` `set` inserts Bevy 0.20's required `Pickable`, and the test spawns the window-less primary window `dev --headless` uses (mesh-picking 5a)); same for a `<mesh2d>` (after stage 1)
- [x] 2.6 `examples/shapes-2d`: a sprite row (plain, tinted, flipped); click a sprite to tint it; `docs/2d.md` rows
- [x] 2.7 The sliced sprite in shapes-2d's row, `image_mode={sliced(...)}`, and its `docs/2d.md` row, once bevy-ui-coverage's helpers return Bevy's reflected shape

## 3. Stage 3: atlases and 2D text (after reflect-coverage stage 1 and bevy-ui-coverage stages 2 and 7)

- [x] 3.1 `bevy-ts/bevy` (types.rs hand entry): `TextureAtlasLayout.fromGrid({ tileSize, columns, rows, padding?, offset? })` returning the asset value `{ $of, props: { fromGrid } }`, typed
- [x] 3.2 `bevy_ts::atlas_layout`: `TextureAtlasLayout` hand-mapped in `reflect_asset`'s builtins, building `from_grid`, cached with the script asset values; positive `tileSize`/`columns`/`rows` checked, naming the prop
- [x] 3.3 The reflect writer's nested-handle hook (reflect-coverage stage 1): a `Handle<TextureAtlasLayout>` field given a `fromGrid` value gets the cached handle; `texture_atlas.index` past the tile count fails naming index and count
- [x] 3.4 `ui.rs`: `<image atlas>` `layout` takes the same value through 3.2's hook; UI's `AtlasLayouts` and `{ tile }` form removed, callers moved to `fromGrid`
- [x] 3.5 `ui.rs`: `<text2d>` kind sharing `<text>`'s props parser and bound-children writer, plus `anchor`, `bounds`, and `textShadow` as `<text>`'s writing `Text2dShadow`; no `Node`; `<span>` children valid in it
- [x] 3.6 `jsx-runtime.d.ts`: `Text2dProps`; `elements.tsx` cases
- [x] 3.7 `two_d.rs`: two sprites with one `fromGrid` value share a 32-tile layout handle; a UI image shares it; an out-of-range index fails; `<text2d>` with a bound child writes only `Text2d` on change, with `TextFont`, `TextColor`, `Anchor` as given
- [x] 3.8 `examples/shapes-2d`: a static atlas frame and a bound score in `<text2d>`; `docs/2d.md` rows

## 4. Stage 4: sprite-sheet animation (after reflect-coverage stage 3)

- [x] 4.1 `examples/shapes-2d`: a TS system on a `Timer` advancing `sprite.texture_atlas.index`, as Bevy's `sprite_sheet` example
- [x] 4.2 Test: at 0.1 s per frame, after 0.35 s of fixed steps the index is 3
- [x] 4.3 `docs/2d.md`: the pattern and its row

## 5. Stage 5: tilemap chunks

- [x] 5.1 `Image.load` takes Bevy's `array_layout`; `TilemapChunk.tileset` refuses a path or a load without it (design 7)
- [x] 5.2 `<TilemapChunkTileData value>` takes an array of `{ tileset_index, color?, visible? }` or `null`; a length other than `chunk_size.x * chunk_size.y` fails naming both
- [x] 5.3 A changed prop on `TilemapChunk` (immutable) re-inserts it, re-running its hook; `TilemapChunkTileData` is written in place
- [x] 5.4 `two_d.rs`: a 4x4 chunk with 16 tiles mounts with both components and an array-texture tileset
- [x] 5.5 `examples/shapes-2d`: a small tile floor; `docs/2d.md` rows

## 6. Landing (each stage)

- [x] 6.1 Rebase the `jsx-runtime` delta on that requirement's text on main at landing (mesh-primitives and bevy-ui-coverage change it too); add only this stage's intrinsics
- [x] 6.2 Protocol unchanged, or the next free minor if a change shows
- [x] 6.3 `docs/2d.md`: no row marked working without a test that exists
