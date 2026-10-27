## 1. The size

- [x] 1.1 `crates/bevy-ts-cli/src/headless.rs`: `HeadlessSize(pub UVec2)` (`Resource`, `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`), `Default` from `SIZE`; doc `SIZE` as the default. `HeadlessPlugin::build` reads `HeadlessSize` if present, else the default, inserts the one it used, and makes the image at it. `HeadlessPlugin` stays a unit struct
- [x] 1.2 Same file: `pub fn fits(size: UVec2, max: u32) -> Result<(), String>`, an error starting `BEVY_TS_SIZE=<W>x<H>:` naming the limit; `HeadlessPlugin::finish` reads `RenderDevice` from the main world (none: no check), and on an error prints `bevy-ts: <error>` and exits 2. Unit test of `fits` in its `tests` module (at, over, either side)
- [x] 1.3 `crates/bevy-ts-cli/src/cli.rs`: `fn headless_size(value: Option<&OsStr>) -> Result<Option<UVec2>, String>`, shaped as `stall_secs`. In `base_app`'s `Display::Headless` arm, before `HeadlessPlugin`: parse `BEVY_TS_SIZE`, insert `HeadlessSize` when set; a bad value prints `bevy-ts: <error>` and exits 2. Not read for a window or viewport
- [x] 1.4 Unit test of `headless_size` in `cli.rs`'s tests beside `stall_secs`'s: unset, `2880x1620`, ` 640x480 ` trimmed; `2880`, `0x540`, `640x0`, `640X480`, `640x480x2`, `-1x5`, `ax b`, empty each fail starting `BEVY_TS_SIZE=`

## 2. Test at a set size

- [x] 2.1 New module `crates/bevy-ts-cli/tests/it/headless_size.rs` (+ `mod headless_size;` in `tests/it/main.rs`), macOS only as `screenshot.rs`: an engine built as `screenshot.rs`'s `engine` (headless default plugins, `HeadlessSize(UVec2::new(640, 480))` inserted before `HeadlessPlugin`, `BevyTsPlugin`, `TsxPlugin`, `RemotePlugin`, `bevy_ts_remote::add_methods`, a `Camera3d`), scratch folder from `common::project`, no env var
- [x] 2.2 Test: the `Offscreen` image is 640x480; a UI root `Node` 100% by 100% has `ComputedUiRenderTargetInfo::logical_size()` 640x480 and covers (0,0)-(640,480)
- [x] 2.3 Test: `bevy-ts/screenshot` through Bevy's mailbox answers `width` 640, `height` 480, and the PNG it wrote is 640x480
- [x] 2.4 Run the `it` binary's `headless_size`, `headless_ui`, `materials`, `screenshot`, `screenshot_ready` and `ui_viewport` modules, and `bevy-ts-cli`'s unit tests; all pass with no edit to the existing ones. 2026-10-27: 95 unit and 50 of 51 module tests pass; `screenshot_ready::stacked_instances_at_one_depth_are_the_same_every_shot` fails the same way on main's code without this change (shot 2 differs in 354944 of 2073600 bytes), so it is not this change's
- [x] 2.5 By hand, a headless `play` of `examples/cube` built with `bevy-ts build`: `BEVY_TS_SIZE=2880x1620 BEVY_TS_SCREENSHOT=<tmp>/shot.png` gives a 2880x1620 PNG; `BEVY_TS_SIZE=2880` exits 2 naming the variable. Record both in this task. 2026-10-27, M5 Max: `bevy-ts build examples/cube`, then `play <out>/game --headless` wrote a 2880x1620 PNG (`sips`); `BEVY_TS_SIZE=2880` exited 2 with `bevy-ts: BEVY_TS_SIZE=2880: expected <width>x<height> in pixels, each at least 1, as 2880x1620`

## 3. Docs

- [x] 3.1 `docs/build.md` "Headless": the image is 960x540 unless `BEVY_TS_SIZE=<W>x<H>` sets it (headless only; the UI, input and both shots follow it; a bad value or one past the GPU's largest texture exits 2 naming it)
- [x] 3.2 `docs/native.md`: `BEVY_TS_SIZE` in the env list beside `BEVY_TS_STALL_SECS`
- [x] 3.3 No coverage table in `docs/` lists headless behaviour, so no row is added; the spec delta and `headless_size.rs` are its record. No edit to `crates/bevy-ts-remote/src/screenshot.rs` (owned by `screenshot-cold-and-atomic` now)
