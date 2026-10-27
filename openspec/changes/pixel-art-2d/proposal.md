## Why

agent-town's Stardew valley (`~/dev/agent-town/openspec/changes/stardew-valley`, design "Engine foundations") needs a pixel-art 2D engine that bevy-ts lacks. Its survey (`research.md`, "Engine: what bevy-ts has for 2D, and the gaps") found the asset root fixed to `<project>/assets` (`crates/bevy-ts-cli/src/headless.rs:48-69`), so the sprites would have to be copied into a repository, which their licence forbids. It also found no y-sort, no pixel-perfect camera, no project-wide nearest sampling, no bitmap font, no sprite clip and no screen-size marker.

## What Changes

Seven stages, each landable on its own, most needed first:

1. **Asset sources outside the project.** `bevy-ts.json` takes `"assetSources": { "stardew": "~/dev/assets/stardew" }`. Each is a Bevy `AssetSource`, loaded as `stardew://Characters/Abigail.png` in any asset slot. `bevy-ts dev` watches it. A missing folder warns, never stops the game. `bevy-ts build` never copies a source: `game.json` names each source's folder, and a built game whose folder is absent exits at start naming it. `bevy-ts build --copy-sources` copies them into the built game, and only then. `bevy-ts levels check` resolves the prefix.
2. **Y-sort.** `<YSort layer={1} offset={-8} />` on an entity draws it by its base y within its layer: lower on screen, in front. `offset` moves the base from the origin to the feet. It sets the entity's `Transform` z from its y before Bevy's transform propagation, the standard Bevy way, so its children (a shadow, a hat) follow by propagation. `YSort` owns that z: `layer` is the authored part.
3. **Pixel-perfect 2D camera.** `<PixelCamera zoom={2} />` on a `Camera2d`: an integer zoom, an eased move between zooms, and the camera's view snapped to whole screen pixels. Sprites land on whole screen pixels from the whole zoom and nearest sampling; none is moved.
4. **Nearest sampling as a project default.** `bevy-ts.json` `"imageSampler": "nearest"` is Bevy's `ImagePlugin::default_nearest()`, baked into a build. Images with their own sampler keep it.
5. **Bitmap fonts.** An XNA `SpriteFont` (as exported to JSON with its PNG) loads as a `SpriteFont` asset. `<bitmapText font="stardew://Fonts/SmallFont.json">` lays out glyphs as MonoGame does: glyph rects, cropping, bearings, spacing, line spacing, the default character.
6. **Sprite clips.** `<SpriteClip first={0} last={3} fps={10} mode="Repeat" />` steps a sprite's atlas index (`Repeat`, `Once`, `PingPong`). It replaces the hand-written timer pattern as the way `docs/2d.md` animates a sheet.
7. **Screen-size sprites.** `<ScreenSize />` keeps a sprite one size on screen at every zoom: a marker over a building reads the same at 1x and 4x. It sets the entity's `Transform` scale before propagation, in the same pass as y-sort. It stays in the world, y-sorted and pickable.

Across stages:

- One pass in `PostUpdate`, after the TSX reconcile and before `TransformSystems::Propagate` (stage 2, grown by 3 and 7), writes `Transform` z (y-sort), `Transform` scale (screen size) and the pixel camera's projection scale. It writes only a changed value, so still sprites keep Bevy's skip of unchanged trees. Nothing writes `GlobalTransform`.
- The pixel camera's snap is one more system, after propagation and before Bevy's camera update: it shifts the camera's projection, never a transform.
- One bench, `bench/pixel-2d`, times both from stage 2 on.
- `docs/2d.md` gains the rows and sections; `docs/assets.md` the sources and the default sampler; `docs/build.md` sources and `--copy-sources`.
- **Not here:** tilemaps and autotiles (script-2d stage 5); a 2D editor camera in the studio; the studio's asset list showing extra sources (`bevy-ts/assets` lists the default source only; a follow-up, it is a protocol change); text wrapping in `<bitmapText>`; a non-contiguous frame list in `<SpriteClip>`.

## Capabilities

### New Capabilities

- `asset-sources`: extra asset folders named in `bevy-ts.json`, loaded by a `name://` prefix in dev, play and build.
- `pixel-art-2d`: y-sort, the pixel camera, screen-size sprites and sprite clips.
- `bitmap-fonts`: XNA SpriteFont assets and `<bitmapText>`.

### Modified Capabilities

- `reflected-assets`: a project-wide default sampler beside "Image sampler and loader settings" (an added requirement; the existing one is unchanged).

## Impact

Needs the operator's go:

- **Performance (stages 2, 3, 7).** A pass in `PostUpdate`, before propagation, over every `YSort` and `ScreenSize` each frame, plus one camera system after it. Off when no entity has these components. A moving y-sorted entity's tree is re-propagated, as any moving entity's is; a still one is not. `bench/pixel-2d` measures it at 10,000 sprites; the target is under 0.5 ms a frame in release.
- **Owned transform fields (stages 2, 7).** `YSort` owns `Transform` z and `ScreenSize` owns `Transform` scale, as `PixelCamera` owns the projection's scale: a script's write to them is overwritten, and a saved level holds the derived values beside the component that derives them. Bevy has no y-sort (sprites are `Mesh2d` quads sorted by z alone, `bevy_sprite_render-0.20.0/src/mesh2d/material.rs:1102,1191`).
- **New infrastructure (stage 1).** Extra Bevy asset sources, a new `bevy-ts.json` key, and `game.json` naming each source's folder; `bevy-ts build --copy-sources` the one way a source's files enter a built game.
- **Open question (stage 3).** Sprites are not snapped one by one (design 3); the operator confirms that, or asks for a per-sprite snap in the render world.

Nothing else needs a go. No new dependency. No protocol change; if one shows, the next free minor at landing.

- Code: `crates/bevy-ts-cli/src/{project.rs,headless.rs,cli.rs,build.rs,play.rs}` (sources, sampler, `game.json`); `crates/bevy-ts-levels/src/check.rs` (source prefixes); `crates/bevy-ts-material/src/pixel2d/` (y-sort, camera, screen size, clips) and `src/bitmap_text/` (loader, layout), beside `world_text/`; `crates/bevy-ts-tsx` (the `<bitmapText>` intrinsic; `host::frame` ordered before the pass).
- Types: `packages/bevy-ts/jsx-runtime.d.ts` (`BitmapTextProps`); the generated `bevy-ts/bevy` types for `YSort`, `PixelCamera`, `ScreenSize`, `SpriteClip`.
- Docs: `docs/2d.md`, `docs/assets.md`, `docs/build.md`, `CHANGELOG.md`.
- Example: `examples/shapes-2d` gains a y-sorted row, a pixel camera, a clip, a marker and a bitmap label (the font drawn with a small font the example ships itself).
- Tests: `crates/bevy-ts-material/tests/it/pixel2d.rs`, `bitmap_text.rs`; `crates/bevy-ts-cli/tests/it/two_d.rs` and `asset_sources.rs`.
- Bench: `bench/pixel-2d`.
