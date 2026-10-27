## MODIFIED Requirements

### Requirement: A headless run renders into one offscreen image
A headless run (`bevy-ts dev --headless`, `bevy-ts play --headless`, `bevy-ts skeleton --headless`) SHALL open no window and SHALL render every camera into one offscreen image, 960x540 unless `BEVY_TS_SIZE` sets its size. `bevy-ts/screenshot` SHALL capture that image when asked. `BEVY_TS_SCREENSHOT=<path>` SHALL save it once as `<path>`, through the same capture code as `bevy-ts/screenshot`; it is for runs that serve no protocol, such as `bevy-ts play --headless`.

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

## ADDED Requirements

### Requirement: BEVY_TS_SIZE sets the headless image's size
`BEVY_TS_SIZE=<W>x<H>` SHALL set the headless image to `W` by `H` pixels, at start-up. `W` and `H` SHALL be whole numbers of at least 1, joined by a lowercase `x`; surrounding space SHALL be ignored. Only a headless run SHALL read it; a window or `--viewport` SHALL ignore it.

The size SHALL belong to the image: every camera, game UI laid out by the default UI camera, the window-less primary window that headless input moves over, `BEVY_TS_SCREENSHOT`'s PNG, and `bevy-ts/screenshot`'s PNG and its `width` and `height` SHALL follow it.

A value that is not `<W>x<H>`, has a side of 0, or has a side larger than the GPU's largest 2D texture SHALL stop the start before the first frame, with exit code 2 and one line on stderr that starts `bevy-ts: BEVY_TS_SIZE=<value>:` and says what is wrong.

#### Scenario: A shot at a larger size
- **WHEN** `bevy-ts play --headless` runs with `BEVY_TS_SIZE=2880x1620` and `BEVY_TS_SCREENSHOT=shot.png`
- **THEN** `shot.png` is 2880x1620

#### Scenario: The protocol's shot at a set size
- **WHEN** a headless engine sized 640x480 is asked for `bevy-ts/screenshot`
- **THEN** the answer's `width` and `height` are 640 and 480, and so is the PNG

#### Scenario: UI fills a set size
- **WHEN** a headless engine sized 640x480 lays out a UI root 100% wide and 100% high
- **THEN** the root's target is 640x480 and the root covers the image from its top-left

#### Scenario: A bad value
- **WHEN** a headless run starts with `BEVY_TS_SIZE=2880` or `BEVY_TS_SIZE=0x540`
- **THEN** it exits with code 2 and a stderr line starting `bevy-ts: BEVY_TS_SIZE=`, and renders no frame

#### Scenario: Unset
- **WHEN** a headless run starts without `BEVY_TS_SIZE`
- **THEN** its image is 960x540
