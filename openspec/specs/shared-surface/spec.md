# shared-surface Specification

## Purpose
Viewport mode: the engine with no window of its own renders into a ring of IOSurfaces another process shows, over a Mach channel per viewer, paced by that viewer's display, behind a virtual primary window game code cannot tell from a real one.

## Requirements

### Requirement: Viewport mode runs the engine with no window of its own
`bevy-ts dev --viewport <entry>` SHALL start the engine with no native window and no winit event loop, load the game as `bevy-ts dev` does, and wait for a viewer to attach. Simulation SHALL run from start; nothing SHALL be rendered until a viewer is attached and visible. `bevy-ts dev` without `--viewport` SHALL keep bridge's behaviour (a window of its own). Viewport mode SHALL be available on macOS only; on other platforms `--viewport` SHALL fail with a message naming the platform.

#### Scenario: Started by abyme
- **WHEN** abyme starts `bevy-ts dev --viewport examples/cube/src/game.ts`
- **THEN** no window or Dock icon appears, `bevy-ts/hello` answers with `viewport: true`, and the cube's systems run

#### Scenario: Started by hand outside abyme
- **WHEN** a developer runs `bevy-ts dev examples/cube/src/game.ts` in a terminal
- **THEN** a window of the engine's own opens, as in bridge

### Requirement: Attaching a viewer
`bevy-ts/viewport_attach` (`w`, `h` in points, `scale`, `hz`, optional `display`, optional `view`) SHALL create a viewport for `view`, which is `"game"` (the default) or `{ preview: <id> }` from `bevy-ts/asset_preview`. It SHALL check in a Mach bootstrap service with a name unique to the process and the attach (`dev.bevy-ts.viewport.<pid>.<n>`), and answer `{ viewport, service, protocol }`. Each viewport SHALL have its own service, ring of surfaces, pacing, input queue and camera target, and viewports of different views SHALL run at the same time. A second attach of a view that already has a viewport SHALL replace that viewport: its old service is ended and its surfaces released, and other views are untouched. `bevy-ts/viewport_detach` (optional `viewport`, default the game viewport) SHALL end that service and release its ring; the game keeps running. A viewer whose reply port dies SHALL be detached as if it had called `viewport_detach` for its viewport. `viewport_resize`, `viewport_input`, `viewport_camera` and `viewport_stats` SHALL take an optional `viewport`, defaulting to the game viewport. The virtual primary window, edit mode, picking and edits SHALL belong to the game viewport alone. A viewport SHALL render only while attached and visible.

#### Scenario: Attach then look up
- **WHEN** a client calls `bevy-ts/viewport_attach` with `w: 800, h: 600, scale: 2, hz: 120`
- **THEN** it gets a viewport id and a service name, and a process in the same GUI bootstrap domain that looks it up and sends `hello` with a reply port receives three `surface` messages of 1600×1200 or larger, then `frame` messages

#### Scenario: The viewer quits
- **WHEN** the viewer's process exits while attached
- **THEN** the engine detaches that viewport within one frame, stops rendering it and keeps simulating

#### Scenario: A preview beside the game
- **WHEN** a client has the game viewport attached and attaches a second viewport with `view: { preview: 3 }`
- **THEN** both services deliver frames at their own viewers' pace, input sent to the preview's channel moves only the preview camera, and the game viewport's frames are unchanged

#### Scenario: A 1.7 client
- **WHEN** a client calls `viewport_attach` without `view`, then `viewport_resize` without `viewport`
- **THEN** both act on the game viewport, as before

### Requirement: One viewer at a time, and the last hello wins
A viewer's `hello` MAY name it as `viewer: { kind, device, mobile }` (`kind`: `app` for abyme.app's surface view, `stream` for abyme-stream; `device` the device that shows it; `mobile` true for a phone). The engine SHALL answer every `hello` on a viewport's channel with its own `hello` `{ inputs }`, listing the `ViewportInput` types that viewport reads (the game viewport all of them; a preview `pointer`, `wheel`, `gesture`, `key` and `visible`). A second `hello` on a viewport that has a viewer SHALL first send that viewer `replaced { viewer }`, naming the new one as its `hello` gave it (`{}` when it named none), and then treat it as gone, as a dead reply port is; the viewport stays attached, and the new viewer gets the ring's `surface` messages again before its first `frame`.

#### Scenario: The phone takes the game
- **WHEN** abyme.app is attached and abyme-stream sends `hello` with `viewer: { kind: "stream", device: "iPhone", mobile: true }` on the same service
- **THEN** abyme.app gets `replaced` naming that viewer and no frame after it, and the stream gets `hello` with `inputs`, three `surface` messages and then frames

#### Scenario: A viewer that names nothing
- **WHEN** a 1.9 viewer sends `hello` with empty params
- **THEN** it is attached as before and answered with `hello` `{ inputs }`, which it may ignore

### Requirement: Frames render into a ring of three shared surfaces
The engine SHALL own a ring of three IOSurfaces (`BGRA`, tagged sRGB, sized in device pixels) and render the viewport's camera output directly into Metal textures created on them, with no copy. Each IOSurface SHALL be sent to the viewer once, as a Mach port from `IOSurfaceCreateMachPort`, in a `surface` message (`layer`, `ring`, `slot`, `w`, `h`). Each finished frame SHALL be announced by a `frame` message (`layer`, `ring`, `slot`, `seq`, `w`, `h`, `ts` in ns on `CLOCK_UPTIME_RAW`) sent only after the GPU has completed every command buffer that writes that slot. The engine SHALL NOT render into the slot it last announced, nor into a slot for which `IOSurfaceIsInUse` is true; when no slot is free, it SHALL skip that frame's render and count it. `layer` names the stacked surface a ring belongs to: 0 is the viewport's picture, the only layer this change makes, and a message without `layer` SHALL mean layer 0; rings, slots and ring numbers are per layer, and a viewer SHALL ignore a layer it does not know.

#### Scenario: No tearing under load
- **WHEN** the game renders a full-screen pattern that changes every frame at 120 Hz for 60 s
- **THEN** no `frame` message names a slot whose rendering had not completed, and captured frames of the viewer show no mixed frames

#### Scenario: The window server still holds a slot
- **WHEN** all three slots are either last announced or in use
- **THEN** the engine skips rendering that frame, `bevy-ts/viewport_stats` reports one more skipped frame, and simulation advances

### Requirement: A virtual primary window stands for the viewport
While a viewer is attached the engine SHALL keep a Bevy `Window` entity marked `PrimaryWindow`, with no native handle, whose physical size is the viewport's pixel size, whose scale factor is the viewer's `scale`, and whose swap chain is the ring's current slot. Cameras that target the primary window SHALL render into the ring with no change to game code. With no viewer attached the window SHALL exist at 1280×720 at scale 2 and nothing SHALL be rendered.

#### Scenario: Game code reads the window
- **WHEN** a TS or Rust system reads the primary window's width and cursor position in viewport mode
- **THEN** it gets the hole's logical width and the pointer's position in the hole, as it would from a real window

### Requirement: Resize without a stretched or stale frame
`viewport_resize` (`w`, `h`, `scale`, over the protocol or the channel) SHALL resize the virtual window at the next frame boundary, coalescing several resizes in one frame to the last. The ring SHALL be allocated at the pixel size rounded up to a multiple of 256 in each dimension; a resize within the current allocation SHALL only change the camera viewport and the `w`, `h` of later `frame` messages, so the viewer crops. A resize outside it SHALL make a new ring with the next `ring` number, send its three `surface` messages before its first `frame`, and release the old ring's surfaces after that first `frame` is sent.

#### Scenario: Dragging a pane edge
- **WHEN** the viewer sends 30 resizes in 500 ms, growing from 800 to 900 points wide at scale 2
- **THEN** at most one new ring is made (1600 to 1800 pixels crosses one 256 boundary), and every frame's `w`, `h` matches the size in force when it rendered

#### Scenario: Moving to a 1x display
- **WHEN** the viewer sends the same points with `scale: 1`
- **THEN** the window's scale factor becomes 1, Bevy sends `WindowScaleFactorChanged`, and frames come at the new pixel size

### Requirement: The viewer's display paces frames
While a viewer is attached and visible, the engine SHALL run one `App::update` per `tick` message (`ts`, `targetTs`, sent by the viewer once per vsync of the hole's display). A tick that arrives while an update is running SHALL start the next update as soon as that one ends; a second tick before then SHALL be dropped. While any viewport is attached and visible, the threads that make a frame (the main thread, the render thread and the compute pool) SHALL run in the user-interactive QoS class, and in their default class otherwise. With no tick for 100 ms while visible, or before the first tick, it SHALL pace itself at `hz`. A `visible` message with `on: false`, or a detach, SHALL stop rendering; play mode SHALL then keep updating at 60 Hz without rendering, and edit mode SHALL update only on protocol requests or reloads.

#### Scenario: A 120 Hz display
- **WHEN** the hole is on a 120 Hz display and the game is light
- **THEN** `bevy-ts/viewport_stats` reports 120 frames a second and no skipped frames

#### Scenario: An update overruns a refresh
- **WHEN** an update takes 9 ms on a 120 Hz display
- **THEN** the tick that arrived during it starts the next update when it ends, frames come at about 110 a second rather than 60, and `droppedTicks` counts only ticks that arrived while one was already waiting

#### Scenario: The pane is hidden
- **WHEN** the viewer sends `visible` with `on: false` in play mode
- **THEN** no `frame` messages are sent, and `Time<Virtual>` keeps advancing at real time

### Requirement: Latency is measured and bounded
`bevy-ts/viewport_stats` SHALL report frames rendered, skipped and dropped ticks, GPU time per frame, and the median and 95th percentile time from an input message's arrival to the `frame` that first reflects it. On this Mac (Apple silicon, 2x display) with the cube example, the median from a key to its `frame` SHALL be at most two display refreshes.

#### Scenario: Key to pixels
- **WHEN** a test viewer sends a key that moves the cube and timestamps the `frame` whose content changed
- **THEN** the median over 200 presses is at most 16.7 ms at 120 Hz

### Requirement: Edit mode and the protocol work in windowed mode
Every viewport method except `viewport_attach`, `viewport_detach`, `viewport_resize` and `viewport_input` SHALL work the same in windowed mode, against the engine's own window.

#### Scenario: Edit mode without abyme
- **WHEN** a client calls `bevy-ts/viewport_mode` with `edit` against `bevy-ts dev` in its own window
- **THEN** the game pauses and the editor camera, gizmos and picking work in that window

### Requirement: The viewport in the protocol contract
The viewport methods (`bevy-ts/viewport_attach`, `viewport_detach`, `viewport_resize`, `viewport_input`, `viewport_mode`, `viewport_camera`, `viewport_pick`, `viewport_select`, `viewport_selection+watch`, `viewport_edits+watch`, `viewport_edit_result`, `viewport_revert`, `viewport_stats`, `assets`, `assets+watch`, `asset_thumbnail`) and the channel's message types (`hello`, sent by both sides, `input`, carrying one `ViewportInput` event, `surface`, `frame`, `tick`, `cursor`, `cursor_grab`, `cursor_warp`, `ime_area`, `ime_allowed`, `replaced`) SHALL be added to `protocol/bevy-ts.openrpc.json` with `x-bevy-ts-crate: "bevy-ts-viewport"`, as an additions-only minor bump of the contract (1.2.0 when it landed; 1.5.0 with the `display` and level-route fields; 1.10.0 with the `hello` answer, `replaced`, `touch` and `gamepad`), generated into Rust and TS types by bridge's pipeline. Channel messages SHALL be JSON-RPC 2.0 notifications whose `method` is the message type, so one schema covers HTTP and the channel. `bevy-ts/hello` SHALL report `viewport: true|false`.

#### Scenario: An old client
- **WHEN** a 1.0 client connects to an engine serving the viewport methods
- **THEN** every 1.0 method answers as before

#### Scenario: A game built without the viewport crate
- **WHEN** a client calls `bevy-ts/viewport_attach` on it
- **THEN** it gets method-not-found
