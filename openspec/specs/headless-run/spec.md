# headless-run Specification

## Purpose
What a run with no window renders into: one offscreen image for every camera and the game's UI, saved by `bevy-ts/screenshot` or `BEVY_TS_SCREENSHOT`.

## Requirements

### Requirement: A headless run renders into one offscreen image
A headless run (`bevy-ts dev --headless`, `bevy-ts play --headless`, `bevy-ts skeleton --headless`) SHALL open no window and SHALL render every camera into one offscreen image, 960x540 unless `BEVY_TS_SIZE` sets its size. `bevy-ts/screenshot` SHALL capture that image when asked. `BEVY_TS_SCREENSHOT=<path>` SHALL save it once as `<path>`, through the same capture code as `bevy-ts/screenshot`; it is for runs that serve no protocol, such as `bevy-ts play --headless`.

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

### Requirement: A script holds a run's ready frames
Every engine SHALL have Bevy resource `ReadyHold`, a set of names, empty at start. A TS system SHALL reach it as param `ReadyHold` from built-in module `bevy-ts/ready`, with:
- `hold(name: string)`: adds `name`; holding a held name changes nothing;
- `release(name: string)`: removes `name`; releasing a name not held changes nothing;
- `isHeld(name: string)`: whether `name` is held.

While any name is held, no frame SHALL be ready, whatever its pipelines and loads, and a frame held SHALL start the ready count again. Under `BEVY_TS_SCREENSHOT` and `BEVY_TS_FIXED_STEP=1`, time SHALL NOT step on a held frame: `TimeUpdateStrategy` stays `ManualDuration(Duration::ZERO)`. Once the last name is released in a frame, the next frame SHALL be ready by the usual rules. A hold set by a `Startup` system of the first generation SHALL hold from the first frame on which the script counts as started, so no step comes before it. Without `BEVY_TS_SCREENSHOT`, `BEVY_TS_FIXED_STEP=1` or a waiting `bevy-ts/screenshot`, a hold SHALL change nothing.

A hold SHALL NOT be progress. When the env shot fails after 60 s with no progress while a name is held, its error SHALL say `held by the script: ` and the held names, joined by `, `, beside anything else not ready. The env shot's error SHALL say everything it waited on: whether the script started, the hold, pipelines waiting, assets loading, and `ready frame <n> of <at>`.

#### Scenario: Data that arrives after the engine starts
- **WHEN** a script holds `"data"` in `Startup`, and releases it in the system that spawns a looping glTF clip once a reply comes, the reply coming after a wall delay of 0 ms in one run and 400 ms with 30 ms of extra work a frame in two more
- **THEN** the three shots are byte-identical

#### Scenario: The same runs without a hold
- **WHEN** a run whose data comes only after its shot does not hold
- **THEN** its shot is taken without the data and differs from the held runs'

#### Scenario: No time passes while held
- **WHEN** a run under the fixed step holds for 20 frames with nothing loading and no pipeline waiting
- **THEN** `Time<Virtual>` elapsed is 0 on each of them, and 1/60 s on the first ready frame after the release

#### Scenario: A hold never released
- **WHEN** a script holds `"abymed"` and never releases it
- **THEN** after the no-progress limit no file is saved, the error names the path and says `held by the script: abymed`, and the run exits with code 1

#### Scenario: A hold in a window
- **WHEN** `bevy-ts dev` with a window, no shot and no fixed step runs a script that holds for ever
- **THEN** time runs as it does without the hold
