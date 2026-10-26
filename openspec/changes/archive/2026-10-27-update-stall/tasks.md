## 1. The watchdog

- [x] 1.1 `crates/bevy-ts-remote/src/stall.rs`: `StallPlugin { limit, action }`, `StallAction` (`Exit(i32)`, `Warn`, `Call`), `StallSettings`, the `Stall` report and its `Update stopped:` line; export from `crates/bevy-ts-remote/src/lib.rs`
- [x] 1.2 The beats: a system at the start of `First`, one at the end of `Last`, one in the render world's `ExtractSchedule`, one after `RenderSystems::Cleanup` in `Render` (wired in `finish`, only when `RenderApp` exists)
- [x] 1.3 The decision as a pure function of the beats and `now`; unit tests in `stall.rs` for the three stuck places and for idle time between frames
- [x] 1.4 The watchdog thread: starts at the first frame, wakes every `limit / 10` (at least 100 ms), reports once per stall, logs "Update resumed after N s" under `Warn`, stops when the app drops
- [x] 1.5 The forwarder: a startup schedule between `PreStartup` and `Startup` swaps `BrpSender`'s sender; a thread keeps each request's result sender and passes it on; on a stall, answer open ones and new ones with `-32603` and the line; before `Exit`, wait for the answers to be read, at most 1 s

## 2. Runs

- [x] 2.1 `crates/bevy-ts-cli/src/cli.rs` `base_app`: add `StallPlugin`, `Exit(75)` headless, `Warn` with a window or viewport, none with `--inspect`
- [x] 2.2 `BEVY_TS_STALL_SECS` read beside `shot_from_env`; `0` turns it off; a bad value fails the start naming the variable; a unit test of the parse in `cli.rs`

## 3. Tests

- [x] 3.1 `crates/bevy-ts-cli/tests/it/common/mod.rs`: `render_app()` and `app()` add `StallPlugin` with the default limit and `Exit(75)`; delete `watchdog` and `Watchdog`
- [x] 3.2 Remove the `common::watchdog` calls in `crates/bevy-ts-cli/tests/it/ui_*.rs` (`ui_viewport` starts from `render_app()`) and `screenshot_ready.rs`'s `Hang` and `hang`
- [x] 3.3 New module `crates/bevy-ts-cli/tests/it/stall.rs` (+ `mod stall;` in `main.rs`): a render blocked by a gate is reported as "in the render" with its frame, after the 1 s limit; a system blocked in `Update` is reported as "the frame's systems"; both with `Call`, never `Exit`
- [x] 3.4 Same module: `bevy-ts/screenshot` posted over HTTP before the render blocks, and `bevy-ts/pause` posted while blocked, both answer an error starting `Update stopped:`
- [x] 3.5 Run the `it` binary's `stall`, `screenshot_ready`, `ui_widgets` and `ui_placement` modules, and `bevy-ts-remote`'s unit tests

## 4. Docs

- [x] 4.1 `docs/build.md`: `BEVY_TS_STALL_SECS` beside `BEVY_TS_SCREENSHOT`, the exit code 75, and the `Update stopped:` line
- [x] 4.2 `docs/native.md`: `BEVY_TS_STALL_SECS` in the env list beside `BEVY_TS_PIPELINE_TRACE`
- [x] 4.3 `docs/testing.md`: an app from `render_app()` or `base_app` is bounded by the engine's stall watchdog, so a test adds none of its own; a stall test sets `Call` and a short limit on the plugin, never an env var
- [x] 4.4 Tell `ts-heap-fields-2` (through the lead) to drop its `common::watchdog` call in `ambient_light.rs` if this lands first (moot: `ts-heap-fields-2` landed first, and this change dropped the call and moved `ambient_light` to `render_app()`)
