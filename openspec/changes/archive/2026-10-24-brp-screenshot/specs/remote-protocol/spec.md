## ADDED Requirements

### Requirement: Methods answered on a later frame
A method whose answer needs later frames SHALL be served with `bevy_ts_remote::add_deferred_method`: registered on Bevy's watching registration under a plain name (no `+watch`), so Bevy's HTTP transport sends exactly one plain JSON-RPC response, the first the handler returns. The contract SHALL list it as an ordinary method (`x-bevy-ts-watch` absent). The handler SHALL return nothing until it has its answer, then the answer once.

#### Scenario: One plain response
- **WHEN** a client posts a deferred method over HTTP
- **THEN** the request stays open until the handler answers, and the reply is one JSON-RPC response, not a stream

### Requirement: Screenshot over the protocol
`bevy-ts/screenshot` (params `path: string`) SHALL save what the run shows on its next rendered frame as a PNG at `path`, by spawning Bevy's `Screenshot` entity observed by Bevy's `save_to_disk(path)`. The target SHALL be the offscreen image under `--headless`, the viewport's frame under `--viewport`, and the primary window in a window. It SHALL answer once the file is written with `{ path, width, height, frame }` (`frame` is the bevy-ts frame it rendered in, the counter `bevy-ts/pause` and `bevy-ts/step` report, which counts paused updates too). It SHALL fail, writing nothing, when `path` is not absolute, does not end in `.png`, or names a folder that does not exist; when no frame renders within 5 seconds, or Bevy started a capture and no image is read back within 60 seconds; and when Bevy captured but did not write the file. Every failure SHALL name the path and the reason. It SHALL work while paused, so a client takes a shot at a chosen frame with `bevy-ts/pause` and `bevy-ts/step` first.

#### Scenario: A shot at a chosen frame
- **WHEN** a headless client pauses, steps the game 30 frames, and calls `bevy-ts/screenshot` with `/tmp/at-30.png`
- **THEN** the answer comes after the file is written, says `width: 960, height: 540` and the frame it rendered in, and the PNG shows the game after those 30 frames

#### Scenario: A folder that does not exist
- **WHEN** the client asks for `/no/such/dir/shot.png`
- **THEN** the call fails with an error naming `/no/such/dir` and no capture is started

#### Scenario: Under the viewport
- **WHEN** a `--viewport` engine with a viewer attached is asked for a shot
- **THEN** the PNG is the frame the viewport shows, at the viewport's pixel size
