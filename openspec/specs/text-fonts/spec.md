# text-fonts Specification

## Purpose
The engine's default font and OS font fallback for `<text>`, the Bevy font features it builds with, and what a shipped game draws with. See `docs/fonts.md`.

## Requirements

### Requirement: Text falls back to the OS's fonts
The engine SHALL build Bevy with `system_font_discovery` beside `default_font`. A `<text>` with no font named SHALL lay out with Bevy's default font first and, for each cluster that font lacks, the font the OS offers for that cluster's script (CoreText on macOS, fontconfig on Linux, DirectWrite on Windows); an emoji cluster SHALL use the OS's emoji font. The engine SHALL bundle no font file for this. A cluster no available font covers SHALL draw the `.notdef` glyph and SHALL NOT fail layout or the run.

#### Scenario: Mixed text draws without boxes
- **WHEN** a headless engine on macOS mounts `<text>ASCII — → “quoted” · 中文 😀 ✓</text>`
- **THEN** every cluster of its laid-out text has a glyph id other than 0

#### Scenario: A symbol inside a Latin run
- **WHEN** the same text holds → and ✓ between Latin words
- **THEN** each draws a real glyph, or the test fails naming the character, so the per-script fallback gap is seen rather than shipped

#### Scenario: Linux without the CJK and emoji fonts
- **WHEN** the test runs on Linux without `fonts-noto-cjk` and `fonts-noto-color-emoji`
- **THEN** it fails, naming the characters with glyph id 0 and the two packages to install

#### Scenario: A character no font has
- **WHEN** a `<text>` holds a character no installed font covers
- **THEN** it draws a box for that character and the rest of the text lays out as usual

### Requirement: Linux engines build and link against fontconfig
The Linux release engines SHALL build with `libfontconfig1-dev` installed and SHALL link `libfontconfig.so.1`. macOS and Windows engines SHALL add no build dependency.

#### Scenario: A Linux release build
- **WHEN** the release workflow builds `linux-x64` and `linux-arm64`
- **THEN** the Linux build step installs `libfontconfig1-dev` and the engine builds

### Requirement: A shipped game draws with the player's fonts
A shipped game SHALL draw text that its own font lacks with what the player's OS has, so the look of such text MAY differ between machines. `docs/fonts.md` SHALL say so, SHALL name shipping a font under `game/assets/` as the way for one game to fix its look, and SHALL name bundling OFL Noto in the engine (with its size per engine) as the opt-in not built.

#### Scenario: Reading what a shipped game gets
- **WHEN** a game author reads `docs/fonts.md`
- **THEN** it says text falls back to the player's OS fonts, that a minimal Linux install may draw CJK and emoji as boxes, and how a game ships its own font
