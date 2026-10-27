## Context

agent-town's Stardew valley asks bevy-ts for eight engine pieces (`~/dev/agent-town/openspec/changes/stardew-valley/design.md`, "Engine foundations"). Tilemaps (5) are script-2d stage 5's and the studio's 2D camera (9) is out of scope; this change is the other seven. The sprites live in `~/dev/assets/stardew` (unpacked with StardewXnbHack: PNGs, and JSON beside each font) and may never enter a repository.

Bevy 0.20 sources below are the crates.io sources (`~/.cargo/registry/src/index.crates.io-*/<crate>-0.20.0/`) and Bevy's examples (`~/dev/refs/bevy-0.20`). MonoGame is `~/dev/refs/MonoGame` at `e23c64c`.

## Goals / Non-Goals

**Goals:** Bevy's own mechanism wherever Bevy has one; each stage lands alone, in the proposal's order; nothing touches saved levels or the protocol.

**Non-Goals:** tilemaps and autotiles (script-2d); a low-resolution render target; text wrapping; a studio 2D camera; the asset list showing extra sources.

## Decisions

### 1. Asset sources are Bevy's `AssetSource`s, named in `bevy-ts.json`

Bevy names sources by a prefix (`name://path`, `bevy_asset-0.20.0/src/path.rs:46`). `AssetSourceBuilder::platform_default(path, None)` (`src/io/source.rs:303`) builds a file reader, writer and watcher for a folder. `App::register_asset_source` (`src/lib.rs:580`) must run before `AssetPlugin` builds, or Bevy logs an error and drops it (`src/lib.rs:631`). So `base_app` (`crates/bevy-ts-cli/src/cli.rs:232`) registers each source before it adds the default plugins, from `ProjectConfig` (`crates/bevy-ts-cli/src/project.rs:68`), which gains `assetSources`. The folder is canonicalised first, as the default root is (`headless.rs:64-67`). `FileAssetReader::new` joins the path to the executable's folder (`src/io/file/mod.rs:49`), and joining an absolute path replaces it, so the folder is always passed absolute.

Dev registers with the watcher and play without, as `asset_plugin` and `asset_plugin_unwatched` differ today (`headless.rs:52-61`). A missing folder warns rather than stops, because one `bevy-ts.json` (agent-town's) serves machines with and without the sprites; the valley tests skip where the folder is absent.

`bevy-ts build` copies each source to `sources/<name>/` beside `assets/` (`build.rs:383-384` copies `assets/` today) and `GameFile` lists the names. Copying into a built game is not copying into a repository: the build folder is the user's, outside version control. A source absent at build time fails the build, as a game that ships without its sprites is broken.

Alternatives: a symlink inside `assets/` (rejected: a link in the repository still points the tooling, the asset list and the build at the sprites, and breaks on other machines); an environment variable for the root (rejected: one root, and invisible in the project file).

### 2. One idempotent post-propagation pass for y-sort, snap and screen size

Sprites sort by `GlobalTransform` z alone (`bevy_sprite_render-0.20.0/src/render/mod.rs:502`: `sort_key = FloatOrd(transform.translation().z)`). Bevy has no y-sort. Two places could write z:

- **`Transform`, before propagation.** Rejected: `Transform` is what scripts, the reconciler and saved levels hold. Draw order would leak into level files and fight every prop write.
- **`GlobalTransform`, after propagation.** Chosen. A system in `PostUpdate`, after `TransformSystems::Propagate`, before `VisibilitySystems::CheckVisibility`.

Bevy skips unchanged trees (`bevy_transform-0.20.0/src/systems.rs:105-111`, `mark_dirty_trees`): an entity whose `Transform` did not change keeps last frame's `GlobalTransform`, already adjusted. So the pass keeps a component per adjusted entity, `Pixel2dApplied { base, written }`. When the current `GlobalTransform` equals `written`, propagation did not touch it and `base` stands; otherwise the current one is the new `base`. The pass computes the adjusted value from `base` alone, writes it and stores it as `written`. That makes it idempotent. The same holds for descendants: an adjusted entity's descendants get `G' · G⁻¹` applied (for y-sort a z shift, for screen size a scale about the origin), with their own `Pixel2dApplied`.

Order inside the pass: screen size (scale), then y-sort (z), then snap (x, y). The pass is skipped by a run condition when no `YSort`, `ScreenSize` or `PixelCamera` exists.

Y-sort's mapping: drawn z = z − clamp(y + offset, ±49,999) / 100,000. The band is under ±0.5, so whole-number layers never mix; at z up to 100, f32's step (about 7.6e-6) is finer than one pixel's 1e-5. Camera2d's default near and far (±1000) hold every layer a game uses.

Bevy's sprite picking reads `GlobalTransform`, so it hits what the pass drew. Picking runs in `PreUpdate`, on last frame's values, which are the drawn ones.

### 3. The pixel camera is integer `scale` plus snapping, not a low-resolution target

Bevy's `pixel_grid_snap` example renders to a 160×90 image and scales it up with a second camera (`examples/2d/pixel_grid_snap.rs:78-101`), fitting by whole multiples (`:116-127`, `projection.scale = 1. / h_scale.min(v_scale).floor()`). That snaps everything for free but fixes the world's resolution and makes the UI, picking and screenshots pass through a canvas. Stardew draws at the window's resolution with a whole-number zoom. So `PixelCamera` takes the example's whole-number rule and applies it to `OrthographicProjection::scale` (`bevy_camera-0.20.0/src/projection.rs:625`) under the default `ScalingMode::WindowSize` (`:523-534`): scale `1/zoom` is `zoom` logical pixels per world pixel. On a 2x display that is `2·zoom` physical pixels, still whole.

Snapping rounds drawn x and y to the step `scale` (the current scale, so mid-ease frames snap to their own step). With an odd target width the centre falls between pixels, so the camera takes half a step on that axis. Snapping covers every `Sprite` (and `<bitmapText>` glyphs, which are sprites); `Mesh2d` and `Text2d` are not snapped (they are not pixel art).

The ease is a cubic ease-out over `ease` seconds of real time, so it still runs with virtual time paused (a paused game can still zoom).

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

`bench/pixel-2d` (outside the gate's build, `docs/testing.md` rule 8) runs one release app headless with 10,000 sprites, each with a child, in arms: `off`, `ysort-still`, `ysort-moving`, `all-moving` (y-sort, snap at zoom 4, a tenth with `ScreenSize`). It reports the pass's median and p95 time per frame. It runs in a check slot (`scripts/slot.sh`), as `bench/world-text/run.sh` does. The target is under 0.5 ms a frame for `all-moving`; past it, the stage says so and the operator decides.

## Risks / Trade-offs

- [Writing `GlobalTransform` after propagation is a new pattern] → kept in one pass, with `Pixel2dApplied` making it idempotent; a test runs 100 still frames and checks nothing drifts.
- [Another system reading `GlobalTransform` in `PostUpdate` before the pass sees undrawn values] → the pass runs right after propagation; documented.
- [Per-frame cost on large scenes] → off without the components; the bench; descendants walked only under adjusted entities.
- [A built game ships the source's files] → documented in `docs/build.md`; the licence is the builder's call, and no repository holds them.
- [The `json` loader shows as the loader of every `.json` in the asset list] → the asset list reports the loader by extension; documented, harmless.
- [The default sampler also turns TTF text's glyph atlas nearest, if Bevy's text atlas uses `Default`] → task 4.3 checks it and documents what holds.

## Migration Plan

Additive. A project without the new keys and components behaves as today. Each stage lands and rolls back alone.

## Open Questions

None blocking. The three items in the proposal's Impact need the operator's go before their stage starts.
