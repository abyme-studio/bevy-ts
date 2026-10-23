## MODIFIED Requirements

### Requirement: A headless run renders into one offscreen image
A headless run (`bevy-ts dev --headless`, `bevy-ts play --headless`, `bevy-ts skeleton --headless`) SHALL open no window and SHALL render every camera into one 960x540 offscreen image. `bevy-ts/screenshot` SHALL capture that image when asked. `BEVY_TS_SCREENSHOT=<path>` SHALL save it once, at frame 60, as `<path>`, through the same capture code as `bevy-ts/screenshot`; it is for runs that serve no protocol, such as `bevy-ts play --headless`.

#### Scenario: A screenshot without a window
- **WHEN** the arena plays with `--headless` and `BEVY_TS_SCREENSHOT=shot.png`
- **THEN** `shot.png` is a 960x540 image of the game camera's view, and no `shot.png.2.png` is written

#### Scenario: A screenshot when asked
- **WHEN** `bevy-ts dev --headless` runs and a client calls `bevy-ts/screenshot`
- **THEN** the PNG is the 960x540 offscreen image at the frame the answer names
