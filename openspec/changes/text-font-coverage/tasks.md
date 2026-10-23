## 1. Engine feature

- [x] 1.1 Add `system_font_discovery` to the `bevy` features in `crates/bevy-ts-cli/Cargo.toml`; keep `default_font`
- [x] 1.2 Add `libfontconfig1-dev` to the Linux build step in `.github/workflows/release.yml`
- [x] 1.3 Measure headless engine startup before and after; report both numbers (first fallback layout, 8 alternated runs: median 293 ms without, 280 ms with; `docs/fonts.md`)

## 2. Test

- [x] 2.1 `crates/bevy-ts-cli/tests/text_fonts.rs`: mount `<text>ASCII — → “quoted” · 中文 😀 ✓</text>` headless, run layout, assert every laid-out cluster has a glyph id other than 0, naming each failing character
- [x] 2.2 On Linux, the failure message names `fonts-noto-cjk` and `fonts-noto-color-emoji`
- [x] 2.3 If → or ✓ fail on macOS, add the OS's symbol font as an extra Latin fallback in fontique's collection (no bundled file) and re-run (not needed: both draw on macOS)
- [x] 2.4 Run the test on macOS (CI) and once by hand on Linux and Windows; record which pass (macOS passes; Linux and Windows not run: no docker or Windows machine free, `scripts/linux-fonts.sh` runs Linux, `docs/fonts.md`)

## 3. Docs

- [x] 3.1 `docs/fonts.md`: default font plus OS fallback per platform, the Latin-run symbol gap, the Linux font and fontconfig needs, what a shipped game gets, a game's own font under `game/assets/`, and bundling OFL Noto as the opt-in with its sizes

## 4. Follow-up, after headless-ui-camera lands

- [ ] 4.1 A `BEVY_TS_SCREENSHOT` shot of the same `<text>`, checked by eye once per platform. Does not gate this change.
