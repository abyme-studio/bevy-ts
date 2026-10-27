## Context

`HeadlessPlugin::build` makes one `Image::new_target_texture(SIZE.0, SIZE.1, ..)` and every camera renders into it (`headless.rs:63`). Everything else already reads the image, not `SIZE`:

- UI: headless gives `IsDefaultUiCamera` to the camera on the image; Bevy lays UI out at that target's size.
- Input: `OffscreenInputPlugin` sizes the window-less primary window from the image (`crates/bevy-ts-viewport/src/offscreen.rs:46`).
- Shots: `bevy-ts/screenshot` and `BEVY_TS_SCREENSHOT` capture `ScreenshotTarget`, the image; the reply's `width`/`height` is the captured image's size (`crates/bevy-ts-remote/src/screenshot.rs:418,533`).

So only the image's creation needs the size. Tests read `SIZE` (`headless_ui.rs`, `materials.rs`, `screenshot.rs`, `screenshot_ready.rs`, `two_d.rs`, `ui_viewport.rs`).

## Goals / Non-Goals

**Goals:** set the headless image's size at start-up; fail a bad value clearly; keep 960x540 and every existing test as they are.

**Non-Goals:** resizing while running; a size for a window or `--viewport`; a scale factor (the image is scale 1, as now); a `--size` flag.

## Decisions

1. **The image owns the size.** `HeadlessSize(UVec2)` is only the request, read once in `HeadlessPlugin::build` to make the image. Nothing else reads it; anything wanting the size reads the `Offscreen` image, as Bevy's cameras and UI do. Changing `HeadlessSize` after build does nothing. Alternative: a resource every consumer reads. Rejected: two owners of one size.
2. **A resource, not a plugin field.** `HeadlessPlugin` stays a unit struct, so every test's `add_plugins(headless::HeadlessPlugin)` still builds. The plugin reads `HeadlessSize` if inserted before it, else `HeadlessSize::default()` (`SIZE`), and inserts what it used. Alternative: `HeadlessPlugin { size }`. Rejected: it changes every caller for no gain.
3. **The env var is read in `base_app`, headless only.** Beside `shot_from_env` and `stall_watchdog`, `base_app` parses `BEVY_TS_SIZE` when `display == Display::Headless` and inserts `HeadlessSize` before adding `HeadlessPlugin`. One place reaches `dev`, `play`, `skeleton`. Tests set no env var (`docs/testing.md`), so they insert `HeadlessSize` themselves.
4. **No flag.** A `--size` on `play` and `dev` would be a second owner, parsed in two parsers and threaded through `play::run` and `dev::run` to `base_app`. The env var already reaches all runs, as `BEVY_TS_SCREENSHOT` does.
5. **The parse.** `headless_size(Option<&OsStr>) -> Result<Option<UVec2>, String>` in `cli.rs`, shaped as `stall_secs`: trimmed, `<W>x<H>`, lowercase `x`, both whole numbers of at least 1. An error starts `BEVY_TS_SIZE=<value>:` and says what is expected (`<width>x<height> in pixels, as 2880x1620`); `base_app` prints `bevy-ts: <error>` and exits 2, as for `BEVY_TS_STALL_SECS`.
6. **The GPU limit.** A side above the device's `max_texture_dimension_2d` would panic deep in wgpu. `HeadlessPlugin::finish` (after `RenderPlugin::finish` puts `RenderDevice` in the main world) checks the size against it through a pure `fits(size, max) -> Result<(), String>`; an error names `BEVY_TS_SIZE`, the size and the limit, and the process exits 2. No `RenderDevice` (no renderer): no check.

## Risks / Trade-offs

- [A large image costs GPU memory and time, about 19 MB at 2880x1620] → opt-in; the default is unchanged.
- [`fits` exits from inside a plugin, which a test cannot catch] → the decision is a pure function with a unit test; no integration test passes a too-large size.
- [Tests and docs that say 960x540] → true at the default; the spec says "by default".
