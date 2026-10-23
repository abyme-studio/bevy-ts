## Context

Bevy 0.20 lays out text with parley 0.11, finds fonts with fontique 0.11 and rasterises with swash. `<text>` inserts `TextFont { font_size, ..default() }` (`crates/bevy-ts-tsx/src/ui.rs`), so its font is `Handle::default()`, filled by `default_font` with FiraMono-subset. The engine's Bevy features (`3d`, `bevy_ui_render`, `bevy_remote`, `file_watcher`) leave out `system_font_discovery`, so fontique has no system collection and any cluster FiraMono lacks draws `.notdef`. Research: `docs/research/text-fonts.md`.

## Goals / Non-Goals

**Goals:**
- Common punctuation, symbols, CJK and emoji draw in `<text>` on macOS, Windows and a desktop Linux.
- A test that proves it from glyph ids, headless, without an image compare.
- Docs that say what a shipped game gets and how to make it look the same everywhere.

**Non-Goals:**
- Bundled fonts. Named in the docs as the opt-in; not built.
- The per-node `font` prop (bevy-ui-coverage).
- wasm: bevy-ts ships no wasm target.
- COLRv1 emoji: swash cannot draw it; every OS emoji font we fall back to is sbix, CBDT or COLRv0.

## Decisions

1. Turn on `bevy/system_font_discovery` in the engine crate only. Alternatives: bundling Noto (11 to 19 MB more per engine and npm package, for every game whether it uses those glyphs or not); fontique's `fontconfig-dlopen`, which would drop the link-time dependency but Bevy does not expose it. Keep `default_font`: FiraMono stays the first family, so ASCII looks as it does today.
2. Leave `TextFont` at its default in `<text>`. Fallback is per cluster, so no family list is needed. The font prop from bevy-ui-coverage sets `TextFont.font` per node and still falls back.
3. The test reads glyph ids, not pixels. It mounts one `<text>` under the engine's text plugins, runs layout, and walks the laid-out glyphs of its `TextLayoutInfo`/`ComputedTextBlock`, asserting each cluster's glyph id is not 0. It names the failing character. It needs no camera, so it does not wait for headless-ui-camera. If the glyph runs are not reachable from a headless app, the implementer says so and stops rather than swapping in a pixel test.
4. The known gap. Parley picks one fallback family per script from a sample of that script. A symbol inside a Latin run (→, ✓, ★) falls back to the OS's Latin font, which may lack it, and then draws `.notdef`, a box. The sample holds → and ✓ inside Latin words, so the test fails on a platform where the gap is real, naming the character. If it fails on macOS, the fix in scope is to add the OS's symbol font (Apple Symbols, Segoe UI Symbol, DejaVu Sans) as an extra fallback for the Latin script in fontique's collection, still no bundled file. Each platform is checked by running the test there: macOS in CI, Linux and Windows by hand once.
5. Linux test fonts. On Linux the test needs `fonts-noto-cjk` and `fonts-noto-color-emoji`; without them CJK and emoji fail by design. The test does not skip: its failure message names the packages.
6. The screenshot is a follow-up. headless-ui-camera makes UI lay out against the offscreen camera so `BEVY_TS_SCREENSHOT` shows it. It has not landed; the shot task stays open in this change's tasks and does not gate it.

## Risks / Trade-offs

- Text differs between machines: it looks native, not identical. Mitigation: docs name bundling and the per-game font.
- A minimal Linux install with no CJK or emoji fonts draws boxes there. Accepted; docs say so.
- The Linux engine now links `libfontconfig.so.1`. Every desktop distro has it; a bare container may not, and the engine then fails to start. Accepted for a desktop game engine; noted in `docs/fonts.md`.
- Startup cost: fontique scans the system font database once, lazily, on the first fallback. Small, but unmeasured; the implementer reports headless startup before and after.
- macOS CI runs `macos-latest`; a runner image with fewer fonts would fail the test. Accepted; the failure names the character.
