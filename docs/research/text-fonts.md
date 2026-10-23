# Why `<text>` draws tofu, and how to stop it (2026-10-23)

Recommendation: turn on Bevy's `system_font_discovery` feature in the engine. It adds no bundled
asset and no new crate of ours (fontique's OS backends come in through Bevy), and needs
`libfontconfig1-dev` on the Linux release runners. Bundling fonts is the fallback if a shipped game
must look the same everywhere: about 11 MB more per engine for Latin, symbols and emoji, about
19 MB with Simplified Chinese, all OFL 1.1.

## What happens today

Bevy 0.20 text is not cosmic-text any more. `bevy_text` 0.20 lays out with parley 0.11 and finds
fonts with fontique 0.11; swash rasterises the glyphs.

- `<text>` inserts `TextFont { font_size, ..default() }` (`crates/bevy-ts-tsx/src/ui.rs:326`).
  The default `FontSource` is `Handle::default()`, which the `default_font` feature fills with
  `FiraMono-subset.ttf`, 18 KB, about ASCII only.
- The engine's Bevy features (`3d`, `bevy_ui_render`, `bevy_remote`, `file_watcher` in
  `crates/bevy-ts-cli/Cargo.toml`) pull in `default_font` through `default_platform`, but not
  `system_font_discovery`.
- Without it, fontique has no system collection: a cluster the font lacks finds no fallback
  family, so an em dash, an arrow, CJK or an emoji draws a box.

## Bevy's own fallback

`bevy/system_font_discovery` → `bevy_text/system_font_discovery` → `parley/system` →
`fontique/system`. With it:

- Parley shapes each cluster against the requested families, then the fallback family for the
  run's script and locale (`parley/src/shape/mod.rs:559`). fontique asks the OS which font covers a
  sample of that script: CoreText on macOS, fontconfig on Linux, DirectWrite on Windows
  (`fontique/src/backend/`).
- Emoji clusters also try the generic `emoji` family (`shape/mod.rs`, `select_font`), which the
  OS maps to Apple Color Emoji, Noto Color Emoji or Segoe UI Emoji.
- Colour glyphs render: `font_atlas.rs:201` asks swash for COLR outlines, then bitmaps, then
  plain outlines. swash reads COLRv0 and bitmap strikes (sbix, CBDT), not COLRv1. Apple Color
  Emoji (sbix), distro Noto Color Emoji (CBDT) and Segoe UI Emoji (COLRv0 layers) all work.
- No API change for scripts: `TextFont` stays the default; a game can still name a font or a
  generic family (`FontSource::Generic`), which today only logs an error.

Platforms:

| Target | Backend | Cost |
| --- | --- | --- |
| macOS | CoreText, in the OS | none |
| Windows | DirectWrite, in the OS | none |
| Linux | fontconfig via `yeslogic-fontconfig-sys` | build needs `libfontconfig1-dev`; the binary links `libfontconfig.so.1`, present on every desktop distro. fontique's `fontconfig-dlopen` feature loads it at run time instead; Bevy does not expose it. |
| wasm | none | bevy-ts ships no wasm target (`docs/release.md`), so moot |

Headless: discovery is a font database lookup, not a window or GPU, so it works with
`--headless` and in tests. A CI box has only the fonts installed on it.

Known gap, medium confidence: fallback is one family per script, picked from a sample of that
script. A symbol in a Latin run (→, ✓, ★) falls back to the Latin system font, which may lack it.
Check this in the shot below before calling it done.

## Bundling instead

Sizes are the upstream files (GitHub, 2026-10-23). All OFL 1.1 except Twemoji.

| Font | Covers | Size | Licence |
| --- | --- | --- | --- |
| Noto Sans Regular | Latin, Greek, Cyrillic, punctuation | 0.62 MB | OFL 1.1 |
| Noto Sans Symbols 2 Regular | arrows, shapes, dingbats | 0.67 MB | OFL 1.1 |
| Noto Sans SC Regular (subset OTF) | Simplified Chinese; JP/KR/TC are one more file each | 8.3 MB | OFL 1.1 |
| Noto Color Emoji (CBDT) | emoji | 10.7 MB | OFL 1.1 |
| Noto COLRv1 | emoji | 5.0 MB | OFL 1.1, but swash cannot draw COLRv1 |
| Twemoji Mozilla (COLRv0) | emoji | 1.5 MB | CC-BY 4.0 art, needs attribution |

`include_bytes!` puts them in each engine binary (the release engine is about 226 MB unstripped).
Every `@bevy-ts/<platform>` npm package grows by the same amount: Latin + symbols + Noto emoji is
about 12 MB a platform, 20 MB with SC. Loaded at startup, they would be registered in `FontCx`
with `set_*_family` and fallback keys, or set as the default `<text>` font list.

## Shipped games

- Fallback: a player sees what their OS has. macOS and Windows cover Latin, symbols, CJK and emoji
  out of the box. A minimal Linux install may lack CJK or emoji and draws boxes there. Text looks
  native, so it differs between machines.
- Bundle: identical everywhere, at the size above per engine, whether the game uses those glyphs
  or not.
- Middle way, no engine change: a game that needs a look ships its own font under `game/assets/`
  and names it; fallback still covers what that font lacks. bevy-ui-coverage's stage 2 adds a
  `font` asset-path prop on `<text>`/`<span>`; it sets `TextFont` per node and leaves the default
  and the feature list to this change.

## Testing it headless

One `<text>` holding `ASCII — → “quoted” · 中文 😀 ✓` under `BEVY_TS_SCREENSHOT`, then:

- read the run's `ComputedTextBlock`/glyph runs and assert every cluster has a glyph id other than
  0 (`.notdef`). This is exact and needs no image compare;
- keep the shot as evidence, checked by eye once per platform.

This needs headless-ui-camera to land first: today headless gives no camera the window, so UI
roots lay out against a zero viewport and are missing from `BEVY_TS_SCREENSHOT`. That change makes
the highest-order offscreen camera the default UI camera, so the text appears in the shot.
macOS CI (`ci.yml` runs `macos-latest`) has every class. On Linux runners, install
`fonts-noto-cjk fonts-noto-color-emoji` for the test, or the CJK and emoji clusters fail there by
design.
