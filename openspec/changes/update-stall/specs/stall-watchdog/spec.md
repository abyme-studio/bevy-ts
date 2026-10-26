## ADDED Requirements

### Requirement: A frame that does not finish is a stall
An engine SHALL watch each frame in two parts, from a thread that is neither the main thread nor the render thread:
- the main part, from the start of `First` until the frame's render extract has run;
- the render part, from the end of that extract until the render world's `Render` schedule has finished that frame. An app with no render world has only the main part.

A part open for the limit, in wall time, SHALL be a stall. Time between frames (an app no one is updating) SHALL NOT count. The clock SHALL start with the first frame, so startup does not count.

#### Scenario: A render that never returns
- **WHEN** a frame's render blocks and the limit passes
- **THEN** the watchdog reports a stall, though the main thread is blocked too

#### Scenario: An app left idle between updates
- **WHEN** a test updates an app, then does other work for longer than the limit before the next update
- **THEN** no stall is reported

### Requirement: A stall says that Update stopped and where
A stall SHALL be reported on stderr in one line that starts `Update stopped:` and names:
- the frame, how long ago it started, and the limit;
- where it is stuck: in the frame's systems (a system or a script is not returning), waiting for the render world (the previous render has not finished), or in the render (the GPU is not taking the engine's work);
- the name of the thread that built the app, when it has one (a test's name under `cargo test`).

When frames come back after a stall that did not exit, the engine SHALL log once that Update resumed and after how long.

#### Scenario: The GPU driver stops completing work
- **WHEN** the render part of frame 812 has been open for 30 s
- **THEN** stderr has a line starting `Update stopped:` that names frame 812 and says the render has not finished

#### Scenario: A script in an endless loop
- **WHEN** a system never returns inside frame 40
- **THEN** the line names frame 40 and says the frame's systems have not finished

### Requirement: What a stall does in each run
On a stall, a headless engine (`--headless`, so every `BEVY_TS_SCREENSHOT` run) and every test app SHALL exit the process with code 75 once it has answered the open protocol requests. An engine with a window or `--viewport` SHALL only report it. An engine run with `--inspect` SHALL have no watchdog. The limit SHALL be 30 s; `BEVY_TS_STALL_SECS=<seconds>` SHALL set it for an engine, and `0` SHALL turn the watchdog off. An invalid value SHALL fail the engine's start with an error naming the variable.

#### Scenario: A headless shot run on a stalled GPU
- **WHEN** `bevy-ts play --headless` with `BEVY_TS_SCREENSHOT` stalls in the render
- **THEN** it exits with code 75 about 30 s after the frame started, with the `Update stopped:` line, and writes no shot

#### Scenario: A window
- **WHEN** a windowed `bevy-ts dev` stalls for 30 s, then its frames come back
- **THEN** it logs the stall, keeps running, and logs that Update resumed

#### Scenario: The limit set to 0
- **WHEN** an engine runs with `BEVY_TS_STALL_SECS=0`
- **THEN** no watchdog thread starts

### Requirement: Tests are bounded by the engine's watchdog
Every test app that renders (`common::render_app()`) and every app built from `base_app` SHALL carry the watchdog with the 30 s limit, so a render test needs no watchdog of its own. A test of the watchdog itself SHALL set a shorter limit and its own action on the plugin, never an environment variable, and SHALL make the stall without a GPU fault: a system that blocks until the test releases it.

#### Scenario: A stalled render test
- **WHEN** a render test's frame stops in the render
- **THEN** the test binary exits within 30 s of that frame, naming the test's thread and `Update stopped:`
