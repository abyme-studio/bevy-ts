## ADDED Requirements

### Requirement: Input events are protocol types carried on the channel
Viewport input SHALL be defined in `protocol/bevy-ts.openrpc.json` as `ViewportInput` events: `pointer` (`kind`: `move`, `down`, `up`, `enter`, `leave`; `x`, `y` in points from the hole's top left; `button`: `left`, `right`, `middle`, `back`, `forward`; `clicks`; `dx`, `dy` raw motion; `mods`), `wheel` (`dx`, `dy`, `unit`: `line` or `pixel`, `phase`, `momentum`, `mods`), `gesture` (`kind`: `pinch`, `rotate`, `double_tap`; `delta`), `key` (`state`: `down`, `up`; `code` as a W3C `KeyboardEvent.code`; `key` logical key; `text`; `repeat`; `mods`), `ime` (`kind`: `enabled`, `preedit`, `commit`, `disabled`; `text`; `cursor` as a UTF-8 byte range), `focus` (`on`) and `visible` (`on`). The viewer SHALL send them as JSON messages on the viewport's Mach channel. `bevy-ts/viewport_input` (an array of events) SHALL accept the same events over HTTP, for tests and for clients without the channel.

#### Scenario: A test drives the game over HTTP
- **WHEN** a test posts `bevy-ts/viewport_input` with a `key` down and up of `KeyW`
- **THEN** a system reading `ButtonInput<KeyCode>` sees `KeyW` just pressed on one frame and just released on a later one

### Requirement: Events become the messages winit would send
The engine SHALL apply queued events at the start of each frame, before `PreUpdate`, in arrival order, as the Bevy 0.20 messages winit's backend writes for the virtual window: `CursorMoved`, `CursorEntered`, `CursorLeft`, `MouseButtonInput`, `MouseMotion`, `MouseWheel`, `PinchGesture`, `RotationGesture`, `DoubleTapGesture`, `KeyboardInput`, `Ime`, `WindowFocused`, and the matching `WindowEvent`s, and SHALL update `Window::cursor_position`. Coordinates SHALL be converted from points to the window's logical pixels. Several `move` events in one frame SHALL each produce a `CursorMoved`. Keys SHALL be released when focus is lost.

#### Scenario: A click on the cube
- **WHEN** the viewer sends `move`, `down` and `up` at the cube's position with `button: left`
- **THEN** a `bevy_picking` `Pointer<Click>` observer on the cube fires once, from the stock mouse pointer

#### Scenario: Focus leaves the hole with a key held
- **WHEN** `KeyW` is down and the viewer sends `focus` with `on: false`
- **THEN** the game sees `KeyW` released and `WindowFocused { focused: false }`

### Requirement: Gamepads are read in the engine, gated by focus
Gamepads SHALL be read by the engine process itself (`gilrs`), not forwarded. While the viewport does not have focus, gamepad input SHALL NOT reach the game unless `bevy-ts/viewport_mode` set `gamepad_unfocused: true`.

#### Scenario: Typing in abyme's editor
- **WHEN** the operator types in a file in abyme while a gamepad is connected
- **THEN** the game receives neither the keys nor gamepad input until the hole has focus again

### Requirement: The engine tells the viewer about cursor and IME
The engine SHALL send the viewer, on the channel: `cursor` (`icon` as a CSS cursor name, or `hidden`) when the virtual window's `CursorIcon` or visibility changes; `cursor_grab` (`mode`: `none`, `confined`, `locked`) when `CursorOptions::grab_mode` changes; `cursor_warp` (`x`, `y`) when game code sets the cursor position; `ime_area` (`x`, `y`, `w`, `h` in points) when `Window::ime_position` changes; and `ime_allowed` (`on`) when `Window::ime_enabled` changes. While `locked`, `pointer` events SHALL still carry raw `dx`, `dy`.

#### Scenario: A first-person game locks the mouse
- **WHEN** the game sets `CursorGrabMode::Locked`
- **THEN** the viewer gets `cursor_grab` with `locked`, and mouse look keeps working from the raw deltas
