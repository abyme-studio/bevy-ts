# Fonts

`<text>` draws with Bevy's default font, FiraMono (about ASCII), and falls back to the OS's fonts
for every character FiraMono lacks: punctuation, arrows, symbols, CJK, emoji. The engine bundles no
font for this. The engine's Bevy features hold `default_font` and `system_font_discovery`
(`crates/bevy-ts-cli/Cargo.toml`). Research: `docs/research/text-fonts.md`.

## How fallback picks a font

Bevy 0.20 lays out text with parley, which finds fonts with fontique and asks the OS:

| OS | Asks | Typical fallback |
| --- | --- | --- |
| macOS | CoreText | the system Latin font, PingFang for Chinese, Apple Color Emoji |
| Windows | DirectWrite | Segoe UI, Microsoft YaHei, Segoe UI Emoji |
| Linux | fontconfig | whatever fontconfig ranks first; Noto or DejaVu on most desktops |

Fallback is per cluster: ASCII keeps FiraMono, so it looks as it always has. Parley picks one
fallback family per script. A symbol inside a Latin run (→, ✓, ★) falls back to the OS's Latin
font, and draws a box if that font lacks it. On macOS the test below holds → and ✓ between Latin
words and every one draws a real glyph. A character no installed font has draws a box (`.notdef`);
the rest of the text lays out as usual.

The first text that needs a fallback makes fontique read the OS's font list, once. On macOS the cost
is lost in the noise. Headless on an M5 Max at load 25 to 40, 8 runs of each build, alternated: the
first layout of the sample took a median 293 ms (222 to 785) without the feature and 280 ms (213 to
354) with it, and a later layout 10 ms either way. Most of that first layout is the frames it spans,
not the scan. A game whose text is all ASCII never scans.

## Linux

The Linux engine links `libfontconfig.so.1`. Every desktop distro has it.

<!-- condition 2: filled from target/linux-fonts/ -->

A minimal Linux install may have no CJK or emoji font, and draws those as boxes. Debian and Ubuntu
packages: `fonts-noto-cjk` and `fonts-noto-color-emoji`. The test below needs both on Linux, and its
failure names them. `scripts/linux-fonts.sh` builds the Linux engine and test in Debian and runs
them in containers with and without fontconfig and these fonts.

## What a shipped game gets

A player sees the fonts their OS has. macOS and Windows cover Latin, symbols, CJK and emoji out of
the box. Text looks native, so it differs between machines.

A game that wants its text to look the same everywhere ships its own font: put the `.ttf` or
`.otf` under the project's `assets/` (it lands in `game/assets/` when built) and name it on its
text with the `font` prop (`docs/ui.md`, from bevy-ui-coverage). Fallback still covers what that
font lacks, so the font should hold every character the game shows.

Not built: bundling OFL Noto in the engine itself, so every game draws the same fallback
everywhere. It would add about 12 MB to each engine and each `@bevy-ts/<platform>` npm package
(Noto Sans, Noto Sans Symbols 2, Noto Color Emoji), 20 MB with Simplified Chinese, whether a game
uses those glyphs or not.

## The test

`crates/bevy-ts-cli/tests/text_fonts.rs` mounts `<text>ASCII — → “quoted” · 中文 😀 ✓ done → next</text>`
under the engine's headless plugins and reads parley's layout from `ComputedTextBlock`: every
cluster must lay out with a glyph id other than 0. A failure names each character drawn as a box.
It reads glyph ids, not pixels, and needs no camera.
