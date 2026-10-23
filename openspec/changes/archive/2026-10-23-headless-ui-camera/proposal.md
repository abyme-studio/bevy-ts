## Why

Headless sends every camera to the offscreen image, so no camera targets the primary window and Bevy picks no default UI camera. UI roots get no target camera, lay out against a zero viewport, and are missing from `BEVY_TS_SCREENSHOT`: agent town's ask card, the arena's Hud.

## What Changes

- Headless gives `IsDefaultUiCamera` to the highest-order camera on the offscreen image, by Bevy's own rule for a window, so UI draws into the image at its size.
- A game's own `IsDefaultUiCamera` on another camera, or a root's `UiTargetCamera`, wins.
- No new API, no protocol change.

## Capabilities

### New Capabilities
- `headless-run`: what a headless run renders into, and what `BEVY_TS_SCREENSHOT` saves.

### Modified Capabilities
None.

## Impact

- `crates/bevy-ts-cli/src/headless.rs`: a `PostUpdate` system before `UiSystems::Prepare`.
- `crates/bevy-ts-cli/tests/headless_ui.rs`: the arena's Hud on the GPU.
- `docs/build.md`: headless and UI.
- Operator go: none needed. A bug fix in Bevy's own terms.
