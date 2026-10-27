# Fonts

`<text>` draws with Bevy's default font, FiraMono (about ASCII), and falls back to the OS's fonts
for every character FiraMono lacks: punctuation, arrows, symbols, CJK, emoji. The engine bundles no
font for this. The engine's Bevy features hold `default_font` and `system_font_discovery`
(`crates/bevy-ts-cli/Cargo.toml`). Research: `docs/research/text-fonts.md`. `<worldText>` lays out
with the same fonts and fallback ([world-text.md](world-text.md)).

## How fallback picks a font

Bevy 0.20 lays out text with parley, which finds fonts with fontique and asks the OS:

| OS | Asks | Typical fallback |
| --- | --- | --- |
| macOS | CoreText | the system Latin font, PingFang for Chinese, Apple Color Emoji |
| Windows | DirectWrite | Segoe UI, Microsoft YaHei, Segoe UI Emoji |
| Linux | fontconfig | whatever fontconfig ranks first; Noto or DejaVu on most desktops |

Fallback is per cluster: ASCII keeps FiraMono, so it looks as it always has. Parley picks one
fallback family per script. A symbol inside a Latin run (→, ✓, ★) falls back to the OS's Latin
font, and draws a box if that font lacks it. The test below holds → and ✓ between Latin
words; it draws them from a fixture font, so a host without them cannot fail it. A character no
installed font has draws a box (`.notdef`); the rest of the text lays out as usual.

The first text that needs a fallback makes fontique read the OS's font list, once. On macOS it
costs nothing measurable: the first layout needing a fallback took a median 293 ms without the
feature and 280 ms with it (8 runs each, headless), most of it the frames the layout spans. A game
whose text is all ASCII never scans.

## Linux

The Linux engine links `libfontconfig.so.1`. Every desktop distro has it. On a machine without it
the engine does not start: fontique reaches fontconfig through `yeslogic-fontconfig-sys`, built
without its `dlopen` feature, whose build script links `fontconfig` found by pkg-config. So the
loader refuses the binary before any of it runs, with `error while loading shared libraries:
libfontconfig.so.1: cannot open shared object file`, exit 127. It fails clearly at launch, never
mid-game. Installing `libfontconfig1` fixes it.

That much is read from the build, not run: the container check is not done yet. To run it, with
docker up:

```sh
scripts/linux-fonts.sh    # logs in target/linux-fonts/
```

It builds the engine and the test in `rust:1-bookworm`, prints the binary's `NEEDED` entries, then
runs the engine in `debian:bookworm-slim` without `libfontconfig1`, and the test with fontconfig and
without and with the Noto fonts (lavapipe as the GPU).

Not done: setting `RUST_FONTCONFIG_DLOPEN=1` in the Linux release build makes that crate load
fontconfig at run time instead of linking it, so the engine would start without it. What fontique
does then without fontconfig, no fallback or a panic, is unchecked.

A minimal Linux install may have no CJK or emoji font, and draws those as boxes. Debian and Ubuntu
packages: `fonts-noto-cjk` and `fonts-noto-color-emoji`. The test below needs both on Linux, and its
failure names them.

## What a shipped game gets

A player sees the fonts their OS has. macOS and Windows cover Latin, symbols, CJK and emoji out of
the box. Text looks native, so it differs between machines.

A game that wants its text to look the same everywhere ships its own font: put the `.ttf` or
`.otf` under the project's `assets/` (it lands in `game/assets/` when built) and name it on its
text with the `font` prop (`docs/ui.md`). Fallback still covers what that
font lacks, so the font should hold every character the game shows.

Not built: bundling OFL Noto in the engine itself, so every game draws the same fallback
everywhere. It would add about 12 MB to each engine and each `@bevy-ts/<platform>` npm package
(Noto Sans, Noto Sans Symbols 2, Noto Color Emoji), 20 MB with Simplified Chinese, whether a game
uses those glyphs or not.

## The test

`crates/bevy-ts-cli/tests/it/text_fonts.rs` mounts `<text>ASCII — → “quoted” · 中文 😀 done ✓ → next</text>`
under the engine's headless plugins and reads parley's layout from `ComputedTextBlock`: every
cluster must lay out with a glyph id other than 0. A failure names each character drawn as a box.
It reads glyph ids, not pixels, and needs no camera.

The punctuation and symbols (— → “ ” · ✓) do not depend on the host: the test registers
`tests/it/fixtures/DejaVuSans-symbols-subset.ttf` (12 KB, DejaVu Sans cut to those six characters;
licence in `DejaVuSans-LICENSE.txt`) in the app's `FontCx` collection as the only fallback for Latin
and Common text (`bevy_text` 0.20 `font.rs`, `register_fonts`; fontique 0.11 `set_fallbacks`), and
asserts each is drawn from that font. The engine itself is unchanged and still falls back to the
OS. CJK and emoji still come from the host's fonts, so the test needs them installed.
