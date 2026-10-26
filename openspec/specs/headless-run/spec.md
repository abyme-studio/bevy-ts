# headless-run Specification

## Purpose
What a run with no window renders into: one offscreen image for every camera and the game's UI, saved by `bevy-ts/screenshot` or `BEVY_TS_SCREENSHOT`.

## Requirements

### Requirement: A headless run renders into one offscreen image
A headless run (`bevy-ts dev --headless`, `bevy-ts play --headless`, `bevy-ts skeleton --headless`) SHALL open no window and SHALL render every camera into one 960x540 offscreen image. `bevy-ts/screenshot` SHALL capture that image when asked. `BEVY_TS_SCREENSHOT=<path>` SHALL save it once as `<path>`, through the same capture code as `bevy-ts/screenshot`; it is for runs that serve no protocol, such as `bevy-ts play --headless`.

It SHALL save at the 60th ready frame in a row. A frame is ready when:
- the script has applied its first generation;
- Bevy's `PipelineCache` had nothing waiting after the render of the frame before, a render that has always finished, so every run reads the same frame's answer;
- no asset load is in flight: no `Handle` an entity's reflected components hold, or an asset added without the server holds, has `RecursiveDependencyLoadState::Loading`;
- every `WorldAssetRoot` and `DynamicWorldRoot` has a `WorldInstance` that `WorldInstanceSpawner::instance_is_ready` reports ready.

A failed asset SHALL NOT keep a frame waiting; the shot SHALL log a warning naming its path. A frame that is not ready SHALL start the count again. If no 60th ready frame comes within 60 s of start, it SHALL save anyway and log a warning saying what is not ready.

Under `BEVY_TS_SCREENSHOT`, time SHALL advance by Bevy's `TimeUpdateStrategy::ManualDuration`: 1/60 s on a ready frame and nothing otherwise. Until the shot is saved, time SHALL stop once it has advanced 60 steps, so the shot's clock is 60 steps even when a ready frame came before a frame that was not. Every camera SHALL draw with Bevy's `NoIndirectDrawing`, given when it is spawned, so the order in which a batch's instances are drawn, and which of two at the same depth shows, is the same every frame. So the shot is the same picture, byte for byte, at any machine load.

#### Scenario: A screenshot without a window
- **WHEN** the arena plays with `--headless` and `BEVY_TS_SCREENSHOT=shot.png`
- **THEN** `shot.png` is a 960x540 image of the game camera's view, and no `shot.png.2.png` is written

#### Scenario: A screenshot when asked
- **WHEN** `bevy-ts dev --headless` runs and a client calls `bevy-ts/screenshot`
- **THEN** the PNG is the 960x540 offscreen image at the frame the answer names

#### Scenario: A script that starts late
- **WHEN** a headless run's script applies its first generation 100 frames after start
- **THEN** the shot shows the script's scene, not an empty frame

#### Scenario: A material whose pipeline compiles late
- **WHEN** a headless run draws a material whose pipeline compiles in the background
- **THEN** the shot shows the material, not the clear color

#### Scenario: A texture on a slow disk
- **WHEN** a headless run draws a cube whose texture's read is held for 100 frames
- **THEN** no shot is saved while it is held, and the shot shows the texture

#### Scenario: The same shot at any load
- **WHEN** a scene moved by elapsed time is shot twice, once with 30 ms of extra work each frame
- **THEN** the two PNGs have identical pixels

#### Scenario: A looping glTF clip at any load
- **WHEN** a glTF scene playing a looping clip is shot twice, once with each read 500 ms slower and 30 ms of extra work each frame
- **THEN** the two PNGs have identical pixels

#### Scenario: A ready frame before a load
- **WHEN** one frame is ready, the next has a pipeline waiting, and the frames after are ready
- **THEN** the shot's clock is 60 steps, not 61

#### Scenario: Instances at one depth
- **WHEN** 64 copies of one textured quad, each turned in the plane they share, are shot four times, two with 30 ms of extra work each frame
- **THEN** the four PNGs have identical pixels

### Requirement: Game UI draws into the headless image
A headless run SHALL give `IsDefaultUiCamera` to the highest-order camera rendering into the offscreen image (ties broken by entity, as Bevy breaks them for a window), and to no other, so UI roots without a `UiTargetCamera` lay out at the image's size and draw into it. When the game places its own `IsDefaultUiCamera` on another camera, headless SHALL place none (one on the camera headless already chose is taken as headless's); a root's `UiTargetCamera` SHALL win as in Bevy.

#### Scenario: The arena's Hud in a screenshot
- **WHEN** the arena plays headless
- **THEN** its Hud's target is the game camera at 960x540, it sits 12 px from the image's top left, and its Spawn button's blue is in the saved image

#### Scenario: A game's own default UI camera
- **WHEN** a headless game spawns a camera with `IsDefaultUiCamera`
- **THEN** UI targets that camera, and headless holds no `IsDefaultUiCamera` of its own until the game's is removed
