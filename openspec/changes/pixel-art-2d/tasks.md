## 1. Stage 1: asset sources (needs the operator's go: new infrastructure)

- [ ] 1.1 `crates/bevy-ts-cli/src/project.rs`: `assetSources` (name to folder; absolute, `~/`, or relative to `bevy-ts.json`); names checked (lowercase, digits, `-`; not `embedded`), errors naming the key and name; resolved to canonical absolute folders
- [ ] 1.2 `headless.rs` and `cli.rs` `base_app`: register each source with `AssetSourceBuilder::platform_default` before the default plugins, watched under dev and unwatched in play (design 1); a missing folder logs one warning naming source and folder
- [ ] 1.3 `build.rs`: copy each source to `sources/<name>/`, list names in `GameFile`; an absent folder fails naming it; `play.rs` registers each listed source from the game folder
- [ ] 1.4 `crates/bevy-ts-levels/src/check.rs`: a `name://` path checked under its source from the `bevy-ts.json` found from the first file; an unknown name is an error naming it and the path
- [ ] 1.5 `crates/bevy-ts-cli/tests/it/asset_sources.rs` (one module of `it`; `common::TestDir` for project and source): a sprite loads `src://a.png` from a folder outside the project; a bad name exits 1 naming it; a missing folder warns and the load fails; build then play loads from `sources/`; `levels check` on a known and an unknown source. Unit tests for path resolution in `project.rs`
- [ ] 1.6 `docs/assets.md`: "Asset sources" section; `docs/build.md`: the copied sources and that the built folder holds them; `CHANGELOG.md` entry

## 2. Stage 2: y-sort (needs the operator's go: per-frame work, a new pattern)

- [ ] 2.1 `crates/bevy-ts-material/src/pixel2d/`: `Pixel2dPlugin`, reflected `YSort { offset }`, `Pixel2dApplied { base, written }`, the pass in `PostUpdate` after `TransformSystems::Propagate` and before `VisibilitySystems::CheckVisibility`, with a run condition (design 2)
- [ ] 2.2 Descendants: `G' · G⁻¹` applied down the tree under an adjusted entity, stopping at a descendant with its own `YSort`
- [ ] 2.3 Engine adds `Pixel2dPlugin`; `bevy-ts/bevy` types `YSort`
- [ ] 2.4 `crates/bevy-ts-material/tests/it/pixel2d.rs`: drawn z for given y and offset, clamp, layer separation, a child's relative z, `Transform` untouched, 100 still frames give one drawn z (the static-tree path)
- [ ] 2.5 Render test (`common::render_app()`): two overlapping y-sorted sprites, red below blue, the overlap pixel is red; swap their y, it is blue
- [ ] 2.6 `crates/bevy-ts-cli/tests/it/two_d.rs`: `<YSort offset={-8} />` from TSX writes `YSort { offset: -8.0 }`
- [ ] 2.7 `bench/pixel-2d` (design 8), arms `off`, `ysort-still`, `ysort-moving`; `run.sh` in a check slot; results in `bench/pixel-2d/results.md`; the stage reports the numbers against 0.5 ms
- [ ] 2.8 `examples/shapes-2d`: a row of y-sorted sprites walking past each other; `docs/2d.md`: "Y-sort" section and coverage row

## 3. Stage 3: pixel camera (needs the operator's go: per-frame work)

- [ ] 3.1 `pixel2d/`: reflected `PixelCamera { zoom, ease }`; `zoom` checked whole and at least 1, naming the value; scale `1/zoom`, cubic ease-out on real time, `ease` 0 jumps (design 3)
- [ ] 3.2 Snap in the pass: the camera to its step with the half step on odd target sizes; every `Sprite` to the step; a second `PixelCamera` warns once naming both
- [ ] 3.3 `pixel2d.rs` tests: scale at zoom 2; mid-ease strictly between, exact at the end; x 10.3 at zoom 4 draws 10.25, `Transform` stays 10.3; odd and even target widths; the warning
- [ ] 3.4 Render test: at zoom 2 a 16 px sprite covers 32 pixels, edges on whole pixels
- [ ] 3.5 Bench arm `all-moving` with snap at zoom 4
- [ ] 3.6 `examples/shapes-2d`: keys 1, 2, 4 set the zoom; `docs/2d.md`: "Pixel camera" section and row

## 4. Stage 4: nearest by default

- [ ] 4.1 `project.rs`: `imageSampler` (`nearest` or `linear`), errors naming the value; `GameFile` carries it
- [ ] 4.2 `base_app`: `ImagePlugin::default_nearest()` when `nearest` (design 4), in dev and play
- [ ] 4.3 Check whether Bevy's text glyph atlas uses `ImageSampler::Default`; write what holds in `docs/assets.md`
- [ ] 4.4 Tests: a project with `nearest` loads an image whose GPU sampler is nearest; an explicit linear sampler stays linear; build then play keeps nearest; an unknown value fails
- [ ] 4.5 `docs/assets.md` (the key, beside per-image samplers), `docs/2d.md` (pixel-art setup), `CHANGELOG.md`

## 5. Stage 5: bitmap fonts

- [ ] 5.1 `crates/bevy-ts-material/src/bitmap_text/`: `SpriteFont` asset and loader for typed `json` loads; the PNG as a dependency from `texture.export`; length checks naming field and counts (design 5)
- [ ] 5.2 Layout as MonoGame's (`SpriteFont.cs:212-258`, `SpriteBatch.cs:616-617`): bearings, spacing, cropping, line spacing, default character, one warning when there is none; measured box for `anchor`
- [ ] 5.3 `BitmapText { text, font, color, scale }` component; glyph children owned, hidden from the reconciler and levels, re-laid on change, removed with it
- [ ] 5.4 `crates/bevy-ts-tsx`: `<bitmapText>` intrinsic (`font`, `color`, `scale`, `anchor`, joined string and bound children); unknown props fail naming prop and line; `jsx-runtime.d.ts` `BitmapTextProps`; `elements.tsx` cases
- [ ] 5.5 `crates/bevy-ts-material/tests/it/bitmap_text.rs`: a small test font (PNG and JSON written by the test) lays `AB` at x 1 and 9; a newline; a missing character; a broken font fails naming `cropping`. Where `~/dev/assets/stardew` exists, `SmallFont.json` loads with the expected counts (skipped, with a line saying so, where it does not)
- [ ] 5.6 `two_d.rs`: a bound `<bitmapText>` goes from two glyphs to three
- [ ] 5.7 `examples/shapes-2d`: a label in a tiny bitmap font the example ships (made for it, no Stardew art); `docs/2d.md`: "Bitmap text" section and row

## 6. Stage 6: sprite clips

- [ ] 6.1 `pixel2d/`: reflected `SpriteClip { first, last, fps, mode, playing, finished }`; system in `Update` on `Time<Virtual>`; restart on a change of `first`, `last` or `mode`; field and atlas errors (design 6)
- [ ] 6.2 `pixel2d.rs` tests: Repeat, Once (`finished`), PingPong at 0.45 s; paused time holds; `last` past the tiles names index and count
- [ ] 6.3 `examples/shapes-2d`: the animated sheet uses `<SpriteClip>`; `docs/2d.md` leads "Atlases" animation with the clip and keeps the timer pattern as the by-hand way; coverage row

## 7. Stage 7: screen-size sprites (needs the operator's go: per-frame work)

- [ ] 7.1 `pixel2d/`: reflected `ScreenSize`; the pass scales by the ruling camera's scale, descendants about its origin (design 2)
- [ ] 7.2 `pixel2d.rs` tests: a 16 px marker covers 16 screen pixels at zoom 1 and 4; picking a click inside its drawn area at zoom 4 runs its handler (picking test as script-2d's, `two_d.rs`)
- [ ] 7.3 Bench: a tenth of `all-moving` with `ScreenSize`
- [ ] 7.4 `examples/shapes-2d`: a marker over a sprite; `docs/2d.md`: "Screen-size sprites" section and row

## 8. Landing (each stage)

- [ ] 8.1 Commits only this stage's paths; the gate's checks run in its slots (`docs/check-slots.md`)
- [ ] 8.2 Protocol unchanged, or the next free minor if a change shows
- [ ] 8.3 `docs/2d.md`: no row marked working without a test that exists
- [ ] 8.4 Rebase on script-2d's `docs/2d.md` and `two_d.rs` at landing; touch none of its stage 5
