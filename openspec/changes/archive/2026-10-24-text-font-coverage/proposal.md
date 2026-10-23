## Why

`<text>` draws a box for an em dash, an arrow, curly quotes, CJK or an emoji. Its only font is Bevy's `default_font`, `FiraMono-subset.ttf` (18 KB, about ASCII), and the engine builds Bevy without `system_font_discovery`, so parley finds no fallback family for what that font lacks (`docs/research/text-fonts.md`).

## What Changes

- The engine turns on `bevy/system_font_discovery` (`crates/bevy-ts-cli/Cargo.toml`). Parley then falls back per script to the font the OS picks (CoreText on macOS, fontconfig on Linux, DirectWrite on Windows), and emoji clusters to the OS emoji font. No bundled font, no script API change.
- Linux release runners install `libfontconfig1-dev`; the Linux engine links `libfontconfig.so.1`.
- A headless test mounts one `<text>` with ASCII, em dash, arrows, curly quotes, middle dot, CJK, emoji and ✓, and asserts no cluster lays out with glyph id 0. On Linux it needs `fonts-noto-cjk` and `fonts-noto-color-emoji`.
- `docs/fonts.md`: a shipped game draws with what the player's OS has; bundling OFL Noto (about 12 MB an engine, 20 MB with Simplified Chinese) is the opt-in for a game that must look the same everywhere. Bundling is not built here.
- The screenshot of that `<text>` waits for headless-ui-camera, which has not landed; it is a follow-up task, not a gate.

## Capabilities

### New Capabilities
- `text-fonts`: the engine's default fonts and fallback for `<text>`, its Bevy font features, and what a shipped game gets.

### Modified Capabilities
None. The per-node `font` prop on `<text>`/`<span>` belongs to bevy-ui-coverage (lead@bevy-ui-coverage); this change owns the default and the feature list only.

## Impact

- `crates/bevy-ts-cli/Cargo.toml`: one Bevy feature. It pulls `fontique/system`, which on Linux brings `yeslogic-fontconfig-sys`, a native link dependency.
- `.github/workflows/release.yml`: `libfontconfig1-dev` added to the Linux build step.
- Linux test hosts: `fonts-noto-cjk fonts-noto-color-emoji`. CI runs no Rust tests on Linux today (`ci.yml` is one macOS job), so this lands as the test's failure message and in `docs/fonts.md`, and goes into any Linux test job when one exists.
- `crates/bevy-ts-cli/tests/text_fonts.rs`: the new test. `docs/fonts.md`: new.
- Operator go needed before build: the fontconfig native link dependency, and the new CI packages (`libfontconfig1-dev` on release runners; the Noto font packages on Linux test hosts).
