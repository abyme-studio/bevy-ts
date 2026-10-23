# headless-run Specification

## Purpose
What a run with no window renders into: one offscreen image for every camera and the game's UI, saved by `bevy-ts/screenshot` or `BEVY_TS_SCREENSHOT`.

## Requirements

### Requirement: A headless run renders into one offscreen image
A headless run (`bevy-ts dev --headless`, `bevy-ts play --headless`, `bevy-ts skeleton --headless`) SHALL open no window and SHALL render every camera into one 960x540 offscreen image. `bevy-ts/screenshot` SHALL capture that image when asked. `BEVY_TS_SCREENSHOT=<path>` SHALL save it once, at frame 60, as `<path>`, through the same capture code as `bevy-ts/screenshot`; it is for runs that serve no protocol, such as `bevy-ts play --headless`.

#### Scenario: A screenshot without a window
- **WHEN** the arena plays with `--headless` and `BEVY_TS_SCREENSHOT=shot.png`
- **THEN** `shot.png` is a 960x540 image of the game camera's view, and no `shot.png.2.png` is written

#### Scenario: A screenshot when asked
- **WHEN** `bevy-ts dev --headless` runs and a client calls `bevy-ts/screenshot`
- **THEN** the PNG is the 960x540 offscreen image at the frame the answer names

### Requirement: Game UI draws into the headless image
A headless run SHALL give `IsDefaultUiCamera` to the highest-order camera rendering into the offscreen image (ties broken by entity, as Bevy breaks them for a window), and to no other, so UI roots without a `UiTargetCamera` lay out at the image's size and draw into it. When the game places its own `IsDefaultUiCamera` on another camera, headless SHALL place none (one on the camera headless already chose is taken as headless's); a root's `UiTargetCamera` SHALL win as in Bevy.

#### Scenario: The arena's Hud in a screenshot
- **WHEN** the arena plays headless
- **THEN** its Hud's target is the game camera at 960x540, it sits 12 px from the image's top left, and its Spawn button's blue is in the saved image

#### Scenario: A game's own default UI camera
- **WHEN** a headless game spawns a camera with `IsDefaultUiCamera`
- **THEN** UI targets that camera, and headless holds no `IsDefaultUiCamera` of its own until the game's is removed
