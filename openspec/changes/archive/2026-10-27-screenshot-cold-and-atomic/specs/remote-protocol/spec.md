## MODIFIED Requirements

### Requirement: Screenshot over the protocol
`bevy-ts/screenshot` (params `path: string`) SHALL save what the run shows as a PNG at `path`. It SHALL capture once Bevy's `PipelineCache` has nothing waiting and no asset load is in flight, as a headless run's ready frame defines both, on the third update in a row that holds, by spawning Bevy's `Screenshot` entity observed by bevy-ts's atomic save: Bevy's `save_to_disk` to a temp file in `path`'s folder, named with a leading dot and unlike `path`, then a rename onto `path`. A reader of `path` SHALL see the old file or the whole new PNG, never part of one. The target SHALL be the offscreen image under `--headless`, the viewport's frame under `--viewport`, and the primary window in a window.

It SHALL answer once the file is written with `{ path, width, height, frame }`. `frame` is the bevy-ts frame it rendered in: the counter `bevy-ts/pause` and `bevy-ts/step` report, which counts paused updates too.

While it waits to capture, it SHALL count progress as a headless run's ready frame does: a pipeline the `PipelineCache` finished, or fewer asset loads in flight than the fewest since the last progress. It SHALL keep waiting while progress comes at least every 60 seconds.

It SHALL fail, writing nothing, when:
- `path` is not absolute, does not end in `.png`, or names a folder that does not exist;
- pipelines are still waiting, or assets still loading, after 60 seconds with no progress;
- no frame renders within 5 seconds;
- Bevy started a capture and no image is read back within 60 seconds;
- Bevy captured but did not write the file.

Every failure SHALL name the path and the reason. It SHALL work while paused, so a client takes a shot at a chosen frame with `bevy-ts/pause` and `bevy-ts/step` first.

With `BEVY_TS_FIXED_STEP=1`, an engine SHALL advance time as under `BEVY_TS_SCREENSHOT`: by `TimeUpdateStrategy::ManualDuration`, 1/60 s on a ready frame (script started, no pipeline waiting, no asset load in flight) and nothing otherwise. Its cameras SHALL draw with `NoIndirectDrawing`, as under `BEVY_TS_SCREENSHOT`. That way a client's shots are the same at any load. Without it, cameras keep Bevy's GPU culling, and two shots of the same frame MAY differ where instances meet at one depth.

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

#### Scenario: A cold build longer than the limit
- **WHEN** a client asks for a shot while pipelines finish one at a time over longer than the no-progress limit, never with a gap as long as it
- **THEN** the call answers after the last pipeline, and the PNG shows every material

#### Scenario: A pipeline that never compiles
- **WHEN** a client asks for a shot while a material's shader read is held for ever
- **THEN** after the no-progress limit the call fails naming the path and the pipelines waiting, and no file is written

#### Scenario: A texture still loading
- **WHEN** a client asks for a shot while a material's texture is still being read
- **THEN** no answer comes while it loads; the shot is taken once it has loaded and the pipelines its material then needs are ready, and shows the texture

#### Scenario: Under load
- **WHEN** a client asks for a shot while the machine is so loaded that the render thread is still drawing the frame before
- **THEN** the method waits for it, and does not fail with "no frame rendered"

### Requirement: Open requests are answered when Update stops
When the stall watchdog fires, an engine SHALL answer every protocol request still open with a JSON-RPC error, code `-32603`, whose message starts `Update stopped:` and is the watchdog's line. That covers a deferred method waiting for frames (such as `bevy-ts/screenshot`) and a request no frame has read yet. An engine that then exits SHALL wait until those answers are read, at most 1 s, before it exits. A request that arrives while the stall lasts SHALL get the same answer.

The method's own timeouts (no frame within 5 s, no readback within 60 s, not ready after 60 s with no progress) SHALL stay as they are: they hold while Update runs. When Update stops, the watchdog's answer is the bound.

#### Scenario: A screenshot asked for before the stall
- **WHEN** a client calls `bevy-ts/screenshot` and the render then stalls
- **THEN** the call fails with a message starting `Update stopped:`, about 30 s after the stalled frame started, and no file is written

#### Scenario: A request sent while stalled
- **WHEN** a client calls `bevy-ts/pause` after the main thread has stopped
- **THEN** the call fails with the same `Update stopped:` message instead of hanging
