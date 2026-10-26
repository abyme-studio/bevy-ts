## MODIFIED Requirements

### Requirement: A headless run renders into one offscreen image
A headless run (`bevy-ts dev --headless`, `bevy-ts play --headless`, `bevy-ts skeleton --headless`) SHALL open no window and SHALL render every camera into one 960x540 offscreen image. `bevy-ts/screenshot` SHALL capture that image when asked. `BEVY_TS_SCREENSHOT=<path>` SHALL save it once as `<path>`, through the same capture code as `bevy-ts/screenshot`; it is for runs that serve no protocol, such as `bevy-ts play --headless`.

It SHALL save at the 60th ready frame in a row. A frame is ready when:
- the script has applied its first generation;
- Bevy's `PipelineCache` had nothing waiting after the render of the frame before, a render that has always finished, so every run reads the same frame's answer;
- no asset load is in flight: no `Handle` an entity's reflected components hold, or an asset added without the server holds, has `RecursiveDependencyLoadState::Loading`;
- every `WorldAssetRoot` and `DynamicWorldRoot` has a `WorldInstance` that `WorldInstanceSpawner::instance_is_ready` reports ready.

A failed asset SHALL NOT keep a frame waiting; the shot SHALL log a warning naming its path. A frame that is not ready SHALL start the count again. If no 60th ready frame comes within 60 s of start, it SHALL save anyway and log a warning saying what is not ready.

Under `BEVY_TS_SCREENSHOT`, time SHALL advance by Bevy's `TimeUpdateStrategy::ManualDuration`: 1/60 s on a ready frame and nothing otherwise. So the shot is the same picture at any machine load.

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
