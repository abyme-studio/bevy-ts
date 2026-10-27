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

A failed asset SHALL NOT keep a frame waiting; the shot SHALL log a warning naming its path. A frame that is not ready SHALL start the count again.

While it waits, the run SHALL count progress: a pipeline Bevy's `PipelineCache` finished, or fewer asset loads in flight than the fewest since the last progress. The shot SHALL wait as long as progress comes at least every 60 s, so a cold build is waited out. If 60 s pass with no progress and no 60th ready frame, it SHALL save nothing, log an error naming the path and what is not ready (pipelines waiting, assets loading, no frame rendered, or the script not started), and the run SHALL exit with code 1.

Once it has captured, it SHALL fail the same way, naming the reason, when no frame renders within 5 s, no image is read back within 60 s, or Bevy captured but did not write the file.

It SHALL write the PNG to a temp file in `<path>`'s folder, named with a leading dot and unlike `<path>`, and then rename it onto `<path>`. A reader of `<path>` SHALL see no file or the whole PNG, never part of one.

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

#### Scenario: A cold build longer than the limit
- **WHEN** six materials' shaders are read one at a time, each after the last one's pipeline is done, so the pipelines finish over longer than the no-progress limit but never with a gap as long as it
- **THEN** the shot is saved after the last pipeline, shows all six materials, and is byte-identical to the same scene shot with no shader held

#### Scenario: A pipeline that never compiles
- **WHEN** a material's shader read is held for ever
- **THEN** after the no-progress limit no file is saved, an error names the path and the pipelines waiting, and the run exits with code 1

#### Scenario: A capture that is never written
- **WHEN** the shot has captured and Bevy writes no file
- **THEN** the run logs an error naming the path and the reason, and exits with code 1

#### Scenario: A reader polling the path
- **WHEN** a large shot is written to a path while another thread reads that path in a loop
- **THEN** every read finds no file or a PNG that decodes whole, and no temp file is left in the folder

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
