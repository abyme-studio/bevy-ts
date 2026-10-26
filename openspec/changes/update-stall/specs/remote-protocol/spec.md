## ADDED Requirements

### Requirement: Open requests are answered when Update stops
When the stall watchdog fires, an engine SHALL answer every protocol request still open with a JSON-RPC error, code `-32603`, whose message starts `Update stopped:` and is the watchdog's line. That covers a deferred method waiting for frames (such as `bevy-ts/screenshot`) and a request no frame has read yet. An engine that then exits SHALL wait until those answers are read, at most 1 s, before it exits. A request that arrives while the stall lasts SHALL get the same answer.

The method's own timeouts (no frame within 5 s, no readback within 60 s, not ready within 60 s) SHALL stay as they are: they hold while Update runs. When Update stops, the watchdog's answer is the bound.

#### Scenario: A screenshot asked for before the stall
- **WHEN** a client calls `bevy-ts/screenshot` and the render then stalls
- **THEN** the call fails with a message starting `Update stopped:`, about 30 s after the stalled frame started, and no file is written

#### Scenario: A request sent while stalled
- **WHEN** a client calls `bevy-ts/pause` after the main thread has stopped
- **THEN** the call fails with the same `Update stopped:` message instead of hanging
