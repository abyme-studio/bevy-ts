## Why

A headless run always renders at 960x540 (`crates/bevy-ts-cli/src/headless.rs:16`). Agent town wants native high-res frames (2880x1620) from `play --headless` with `BEVY_TS_SCREENSHOT` and from `bevy-ts/screenshot`, without upscaling a small shot.

## What Changes

- `BEVY_TS_SIZE=<W>x<H>` (as `2880x1620`) sets the size of the headless image every camera renders into. Unset, it stays 960x540.
- The size belongs to the render target, as in Bevy: cameras, game UI, the window-less primary window that headless input uses, `BEVY_TS_SCREENSHOT` and `bevy-ts/screenshot`'s `width`/`height` all follow the image. None of them reads the setting.
- A bad value (not `WxH`, a zero side, or a side larger than the GPU's largest texture) stops the start with exit code 2 and a line naming `BEVY_TS_SIZE`.
- Only headless runs read it. A window or `--viewport` ignores it.
- `HeadlessSize`, a resource in `crates/bevy-ts-cli/src/headless.rs`, carries the size into `HeadlessPlugin`. `SIZE` stays, as the default.
- No `--size` flag: the variable reaches `dev`, `play` and `skeleton` in one place, beside `BEVY_TS_SCREENSHOT` and `BEVY_TS_STALL_SECS`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `headless-run`: the offscreen image is 960x540 by default and `BEVY_TS_SIZE` sets it; a bad value fails the start.

## Impact

- Operator's go: none needed. Nothing on the ask-first list: no new dependency, service or pattern; the default size, and so default performance, is unchanged; a larger size costs GPU time only for a run that asks for it.
- Code: `crates/bevy-ts-cli/src/headless.rs` (`HeadlessSize`, the plugin reads it, the GPU limit check); `crates/bevy-ts-cli/src/cli.rs` (`base_app` reads `BEVY_TS_SIZE` for headless runs, parse and its unit test).
- No edit to `crates/bevy-ts-remote/src/screenshot.rs`: its reply already reads the captured image's size. `screenshot-cold-and-atomic` is editing that file; this change does not touch it.
- Tests: a new module `crates/bevy-ts-cli/tests/it/headless_size.rs`. Existing tests read `SIZE` and run at the default, unchanged.
- Docs: `docs/build.md` "Headless" and `docs/native.md`'s env list.
- Protocol: no schema change.
