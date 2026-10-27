## Context

agent-town's Stardew valley asks bevy-ts for eight engine pieces (`~/dev/agent-town/openspec/changes/stardew-valley/design.md`, "Engine foundations"). Tilemaps (5) are script-2d stage 5's and the studio's 2D camera (9) is out of scope; this change is the other seven. The sprites live in `~/dev/assets/stardew` (unpacked with StardewXnbHack: PNGs, and JSON beside each font) and may never enter a repository.

Bevy 0.20 sources below are the crates.io sources (`~/.cargo/registry/src/index.crates.io-*/<crate>-0.20.0/`) and Bevy's examples (`~/dev/refs/bevy-0.20`). MonoGame is `~/dev/refs/MonoGame` at `e23c64c`.

## Goals / Non-Goals

**Goals:** Bevy's own mechanism wherever Bevy has one; nothing writes `GlobalTransform`; each stage lands alone, in the proposal's order; no protocol change.

**Non-Goals:** tilemaps and autotiles (script-2d); a low-resolution render target; text wrapping; a studio 2D camera; the asset list showing extra sources.

## Decisions

### 1. Asset sources are Bevy's `AssetSource`s, named in `bevy-ts.json`

Bevy names sources by a prefix (`name://path`, `bevy_asset-0.20.0/src/path.rs:46`). `AssetSourceBuilder::platform_default(path, None)` (`src/io/source.rs:303`) builds a file reader, writer and watcher for a folder. `App::register_asset_source` (`src/lib.rs:580`) must run before `AssetPlugin` builds, or Bevy logs an error and drops it (`src/lib.rs:631`). So `base_app` (`crates/bevy-ts-cli/src/cli.rs:232`) registers each source before it adds the default plugins, from `ProjectConfig` (`crates/bevy-ts-cli/src/project.rs:68`), which gains `assetSources`. The folder is canonicalised first, as the default root is (`headless.rs:64-67`). `FileAssetReader::new` joins the path to the executable's folder (`src/io/file/mod.rs:49`), and joining an absolute path replaces it, so the folder is always passed absolute.

Dev registers with the watcher and play without, as `asset_plugin` and `asset_plugin_unwatched` differ today (`headless.rs:52-61`). A missing folder warns rather than stops, because one `bevy-ts.json` (agent-town's) serves machines with and without the sprites; the valley tests skip where the folder is absent.

`bevy-ts build` never copies a source. The sprites' licence forbids a repository holding them, and a built folder is one copy away from a repository or a shared drive, so their files leave their folder only when asked. `game.json` gains `assetSources`, each name to its folder as `bevy-ts.json` wrote it: an absolute path or `~/` kept as written (`~/` read on the playing machine, as `env`'s `${NAME}` is, `play.rs:37-42`), a relative one made absolute at build time. A folder absent at build time warns naming it; the build goes on.

A built game registers each source from `game.json` before `AssetPlugin` builds. A folder absent there exits 1 at start, before a window opens, with one line naming the source, the folder looked in, and the fix: put the folder there, or build with `--copy-sources`. This differs from dev on purpose: dev serves machines with and without the sprites; a player with a built game and no sprites has a broken game, better told at once than shown blank sprites.

`bevy-ts build --copy-sources` copies each source to `sources/<name>/` beside `assets/` (`build.rs:381-384` copies `assets/` today), and `game.json` then names `sources/<name>` relative to the game folder. A source absent at build time with `--copy-sources` fails the build naming it. The report names each copied source and its file count, so a copy is never silent.

Alternatives: a symlink inside `assets/` (rejected: a link in the repository still points the tooling, the asset list and the build at the sprites, and breaks on other machines); an environment variable for the root (rejected: one root, and invisible in the project file).

### 2. One pass before propagation for y-sort and screen size

Sprites in Bevy 0.20 are `Mesh2d` quads (`bevy_sprite_render-0.20.0/src/sprite_mesh/mod.rs:59-78`), sorted by their world z alone (`mesh2d/material.rs:1102`, `sort_key: FloatOrd(mesh_z + depth_bias)` at `:1191`). Bevy has no y-sort. The standard way is to set `Transform` z from y before propagation, so `GlobalTransform` is Bevy's own and every reader of it (picking, visibility, extraction) sees one value.

The pass is `Pixel2dSystems`, in `PostUpdate`:

- after the TSX reconcile. `host::frame` runs after `BevyTsSet::PostUpdate` (`crates/bevy-ts-tsx/src/lib.rs:523`), and `BevyTsSet::PostUpdate` is before propagation (`crates/bevy-ts/src/schedules.rs:222`). `host::frame` gains `.before(bevy_ts_material::Pixel2dSystems)`, as it has `.before(WorldTextSystems)` today.
- before `TransformSystems::Propagate` (`bevy_transform-0.20.0/src/plugins.rs:37-47`: `mark_dirty_trees`, `propagate_parent_transforms`, `sync_simple_transforms`, chained).

In order: the pixel camera's scale (design 3), then screen size, then y-sort. Each writes a field only when its value changes (Bevy's `set_if_neq`). Propagation recomputes only trees whose `Transform` changed (`bevy_transform-0.20.0/src/systems.rs:111-112`, `mark_dirty_trees` on `Changed<Transform>`), so a still sprite costs a read and a compare, and is not re-propagated. A run condition skips the pass when no `YSort`, `ScreenSize` or `PixelCamera` exists.

Y-sort's formula, for `YSort { layer, offset }`:

    z = layer − clamp(y + offset, −49,999, 49,999) × 10⁻⁵

`y` is the entity's own `Transform` y: its world y when it has no parent, its y in the parent's space when it has one (its z then adds to the parent's, so it sorts among its siblings). `offset` moves the base from the origin to the feet. Range limits:

- Base y is clamped to ±49,999 px, so the y term stays inside ±0.5 and layers a whole unit apart never mix. Past the clamp, sprites tie and draw in an unspecified order; the pass warns once naming the entity and its y.
- `|layer|` is at most 127, so `|z|` stays under 128. There an f32's step is at most 2⁻¹⁷ ≈ 7.6·10⁻⁶, finer than one pixel's 10⁻⁵, so a one-pixel change of y always changes z. A larger `layer` fails naming `layer` and the value.
- Camera2d's default projection draws z from −1000 to 1000 about the camera (`bevy_camera-0.20.0/src/projection.rs:771-786`, `near: -1000.0`, `far: 1000.0`), wider than ±127.5.
- A child's local z is added after: one pixel of y is 10⁻⁵, so a shadow at local z −10⁻⁵ draws behind its parent and in front of anything a pixel or more further up.

`YSort` owns `Transform` z, as `PixelCamera` owns the projection's scale: a script writes `layer`, and a write to z is overwritten the same frame. A level saved from the world holds the derived z beside `YSort`; loading it recomputes z on the first frame, so the saved z never decides anything.

Screen size: `ScreenSize { scale }` sets `Transform` scale x and y to `scale` times the ruling camera's projection scale (z stays 1). Its descendants scale with it by propagation. It owns `Transform` scale in the same way. Under a scaled parent the parent's scale multiplies in; documented.

Picking reads `GlobalTransform` and the camera's projection, both Bevy's own, so a click hits what is drawn.

Rejected: writing `GlobalTransform` after propagation (the first draft). It needed a per-entity record of the unadjusted value, because Bevy skips unchanged trees, and it made the drawn and the propagated transform two things every other system had to know about.

### 3. The pixel camera is integer `scale` plus a snapped view, not a low-resolution target

Bevy's `pixel_grid_snap` example renders to a 160×90 image and scales it up with a second camera (`examples/2d/pixel_grid_snap.rs:78-101`), fitting by whole multiples (`:116-127`, `projection.scale = 1. / h_scale.min(v_scale).floor()`). That snaps everything for free but fixes the world's resolution and makes the UI, picking and screenshots pass through a canvas. Stardew draws at the window's resolution with a whole-number zoom. So `PixelCamera` takes the example's whole-number rule and applies it to `OrthographicProjection::scale` (`bevy_camera-0.20.0/src/projection.rs:625`) under the default `ScalingMode::WindowSize` (`:523-534`): scale `1/zoom` is `zoom` logical pixels per world pixel. On a 2x display that is `2·zoom` physical pixels, still whole.

The scale is written in the pass, before propagation (design 2), so screen size reads this frame's scale. The ease is a cubic ease-out over `ease` seconds of real time, so it still runs with virtual time paused (a paused game can still zoom).

The snap needs the camera's world position, which exists only after propagation. It is one system, `.after(TransformSystems::Propagate).before(CameraUpdateSystems)`. `CameraUpdateSystems` holds Bevy's `camera_system` (`bevy_render-0.20.0/src/camera.rs:75-80`), which recomputes the projection when it changed (`:393`) and runs before `update_frusta`; nothing in Bevy 0.20 orders `CameraUpdateSystems` before propagation, so the order adds no cycle. The system rounds the camera's global x and y to its step (the current scale; half a step more on an axis whose target size in pixels is odd) and shifts the projection's `viewport_origin` (`projection.rs:605`, used at `:729-730`) by the difference, over the projection's size. The view is drawn from the snapped point; the camera's `Transform` and `GlobalTransform` keep the script's value. Picking builds its ray from the same projection, so it agrees with what is drawn.

Sprites are not snapped one by one. At a whole zoom a texel spans exactly `zoom` screen pixels, and the GPU fills a pixel whose centre lies inside the quad, so a sprite at any sub-pixel x covers the same whole pixels as the sprite rounded to the nearest screen pixel, and with nearest sampling each pixel shows one texel. x 10.3 at zoom 4 draws the pixels x 10.25 draws. The one case it does not settle is a texel edge exactly on a pixel centre (a screen fraction of exactly one half), where float error can pick either side; it shows as a one-pixel flicker on a sprite moving at exactly half-pixel steps. A per-sprite snap would have to move each sprite's extracted transform in the render world after `extract_2d_meshes` (`bevy_sprite_render-0.20.0/src/mesh2d/mesh.rs:298-312`, which extracts only `Changed<GlobalTransform>`, so it would also keep each unsnapped value). That is a render-world pass over every sprite; this change does not take it, and Open Questions asks the operator.

Mid-ease, the zoom is not whole and texels are uneven for those frames; the ease is short and that is accepted.

### 4. The default sampler is `ImagePlugin::default_nearest()`

`ImagePlugin` holds `default_sampler`, used by every image whose sampler is `ImageSampler::Default` (`bevy_image-0.20.0/src/image.rs:163-166`); Bevy's default is linear (`:178-181`) and `default_nearest()` sets nearest (`:209-211`). Bevy's own pixel example sets it on `DefaultPlugins` (`examples/2d/pixel_grid_snap.rs:23`). `base_app` sets it the same way. It goes beside the switches in `game.json`, so dev and play cannot differ, as `Switches` does (`project.rs:98-111`). Per-image samplers (`docs/assets.md:102-120`) keep winning, as they are not `Default`.

### 5. Bitmap fonts: a Bevy `AssetLoader`, MonoGame's layout, sprites as glyphs

Bevy's text is parley over TTF/OTF (`bevy_text-0.20.0/src/font.rs:30`); it has no bitmap font. So `SpriteFont` is a bevy-ts asset with its own `AssetLoader`. It registers for `json`; Bevy picks a loader by asset type before extension when the load is typed (`bevy_asset-0.20.0/src/server/loaders.rs:155-170`), so `Handle<SpriteFont>` loads choose it and other JSON stays untouched. The PNG is a dependency loaded from `texture.export` beside the JSON, in the same source.

Layout copies MonoGame: glyph fields from `kerning` (`SpriteFont.cs:85-87`: X left bearing, Y width, Z right bearing), the line walk (`SpriteFont.cs:212-258`) and the cropping offset (`SpriteBatch.cs:616-617`). XNA has no pair kerning: "kerning" is these bearings.

Each glyph is a child entity with a `Sprite` whose `rect` is the glyph's rect: Bevy batches sprites of one image, so a label is one draw. The children are owned as world text owns its mesh (`crates/bevy-ts-material/src/world_text/`): hidden from the reconciler and levels, removed with the element. A Bevy-side `BitmapText { text, font, color, scale }` component is the interface; `<bitmapText>` writes it, as `<worldText>` writes `WorldText`.

Alternative rejected: a TTF made from the sheet (lossy, and loses Stardew's spacing).

### 6. Sprite clips follow Bevy's `sprite_animation` example

Bevy's example holds `first_sprite_index`, `last_sprite_index`, `fps` and a timer (`examples/2d/sprite_animation.rs:33-45`). `SpriteClip` has the same fields as `first`, `last`, `fps`, plus `mode` and `playing`, and a system in `Update` steps it on `Time<Virtual>`. Bevy's `AnimationClip` was rejected: it targets named entities through `AnimationTargetId` and an `AnimationPlayer`, heavy for a frame counter. The hand-written TS timer pattern stays as a test of the reflect write path (`two_d::a_ts_system_on_a_timer_steps_a_sprites_atlas_index`), and `docs/2d.md` leads with the clip.

### 7. Home: `bevy-ts-material`, beside world text

`crates/bevy-ts-material` already holds world text, the engine's other drawing plugin with owned children. `pixel2d/` and `bitmap_text/` go there, each with a plugin the engine adds and a Rust app can add. No new crate.

### 8. The bench

`bench/pixel-2d` (outside the gate's build, `docs/testing.md` rule 8) runs one release app headless with 10,000 sprites, each with a child, in arms: `off`, `ysort-still`, `ysort-moving`, `all-moving` (y-sort, a pixel camera easing between zoom 2 and 4, a tenth with `ScreenSize`). It reports the pass's median and p95 time per frame. It times the pass, the camera snap and Bevy's propagation apart, so a cost moved into propagation shows. It runs in a check slot (`scripts/slot.sh`), as `bench/world-text/run.sh` does. The target is under 0.5 ms a frame for `all-moving`; past it, the stage says so and the operator decides.

## Risks / Trade-offs

- [`YSort` and `ScreenSize` overwrite a script's `Transform` z and scale] → documented as owned fields, as `PixelCamera` owns scale; `docs/2d.md` says to write `layer` and `ScreenSize.scale`.
- [A saved level holds derived z and scale] → recomputed on the first frame after load; a level diff shows them change as y does, documented.
- [A y-sorted entity under a moving parent sorts by its y in the parent's space] → documented; y-sort is meant for roots and for siblings under one still parent.
- [A system reading `Transform` z of a y-sorted entity sees the derived value] → that is the drawn value, which Bevy's own readers want.
- [Per-frame cost on large scenes] → off without the components; only changed values written; the bench.
- [Sprites at exactly half-pixel screen positions may flicker] → no per-sprite snap; Open Questions.
- [A built game without `--copy-sources` needs the source folder on the player's machine] → it says so at start and exits; `docs/build.md` says when to pass the flag.
- [`--copy-sources` puts the source's files in the built folder] → only when asked, named in the build's report; the licence is the builder's call.
- [The `json` loader shows as the loader of every `.json` in the asset list] → the asset list reports the loader by extension; documented, harmless.
- [The default sampler also turns TTF text's glyph atlas nearest, if Bevy's text atlas uses `Default`] → task 4.3 checks it and documents what holds.

## Migration Plan

Additive. A project without the new keys and components behaves as today. Each stage lands and rolls back alone.

## Open Questions

- Stage 3: is the snap by rasterization (design 3) enough, with its one flicker case at exact half-pixel positions? If not, a per-sprite snap goes in the render world after `extract_2d_meshes`, with its own bench arm. Asked of the operator before stage 3 starts.

The proposal's Impact items need the operator's go before their stage starts.
