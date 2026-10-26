## MODIFIED Requirements

### Requirement: Screenshot over the protocol
`bevy-ts/screenshot` (params `path: string`) SHALL save what the run shows as a PNG at `path`. It SHALL capture once Bevy's `PipelineCache` has nothing waiting and no asset load is in flight, as a headless run's ready frame defines both, by spawning Bevy's `Screenshot` entity observed by Bevy's `save_to_disk(path)`. The target SHALL be the offscreen image under `--headless`, the viewport's frame under `--viewport`, and the primary window in a window.

It SHALL answer once the file is written with `{ path, width, height, frame }`. `frame` is the bevy-ts frame it rendered in: the counter `bevy-ts/pause` and `bevy-ts/step` report, which counts paused updates too.

It SHALL fail, writing nothing, when:
- `path` is not absolute, does not end in `.png`, or names a folder that does not exist;
- pipelines are still waiting, or assets still loading, after 60 seconds;
- no frame renders within 5 seconds;
- Bevy started a capture and no image is read back within 60 seconds;
- Bevy captured but did not write the file.

Every failure SHALL name the path and the reason. It SHALL work while paused, so a client takes a shot at a chosen frame with `bevy-ts/pause` and `bevy-ts/step` first.

With `BEVY_TS_FIXED_STEP=1`, an engine SHALL advance time as under `BEVY_TS_SCREENSHOT`: by `TimeUpdateStrategy::ManualDuration`, 1/60 s on a ready frame (script started, no pipeline waiting, no asset load in flight) and nothing otherwise. That way a client's shots are the same at any load.

#### Scenario: A shot at a chosen frame
- **WHEN** a headless client pauses, steps the game 30 frames, and calls `bevy-ts/screenshot` with `/tmp/at-30.png`
- **THEN** the answer comes after the file is written, says `width: 960, height: 540` and the frame it rendered in, and the PNG shows the game after those 30 frames

#### Scenario: A folder that does not exist
- **WHEN** the client asks for `/no/such/dir/shot.png`
- **THEN** the call fails with an error naming `/no/such/dir` and no capture is started

#### Scenario: Under the viewport
- **WHEN** a `--viewport` engine with a viewer attached is asked for a shot
- **THEN** the PNG is the frame the viewport shows, at the viewport's pixel size

#### Scenario: A pipeline still compiling
- **WHEN** a client asks for a shot while a material's pipeline is compiling
- **THEN** the shot is taken once the pipeline is ready, and shows the material

#### Scenario: A texture still loading
- **WHEN** a client asks for a shot while a material's texture is still being read
- **THEN** no answer comes while it loads; the shot is taken once it has loaded, and shows the texture
