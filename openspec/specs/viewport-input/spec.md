# viewport-input Specification

## Purpose
Input from the viewer of a viewport (pointer, wheel, gestures, keys, IME, touch, gamepads, focus) carried on the channel or over HTTP and applied as the messages winit and gilrs would send, with cursor and IME state sent back to the viewer and gamepads also read in the engine.

## Requirements

### Requirement: Input events are protocol types carried on the channel
Viewport input SHALL be defined in `protocol/bevy-ts.openrpc.json` as `ViewportInput` events: `pointer` (`kind`: `move`, `down`, `up`, `enter`, `leave`; `x`, `y` in points from the hole's top left; `button`: `left`, `right`, `middle`, `back`, `forward`; `clicks`; `dx`, `dy` raw motion; `buttons` held; `mods`, with `shift`, `ctrl`, `alt`, `meta` and `caps`), `wheel` (`dx`, `dy`, `unit`: `line` or `pixel`, `phase`, `momentum`, `mods`), `gesture` (`kind`: `pinch`, `rotate`, `double_tap`; `delta`; `x`, `y`; `phase` as `wheel`'s), `key` (`state`: `down`, `up`; `code` as a W3C `KeyboardEvent.code`; `key` logical key; `text`; `repeat`; `mods`), `ime` (`kind`: `enabled`, `preedit`, `commit`, `disabled`; `text`; `cursor` as a UTF-8 byte range), `touch` (`kind`: `start`, `move`, `end`, `cancel`; `touches`, every touch down after the event, each `id`, `x`, `y` in points from the hole's top left, `rx`, `ry` radii in points and `force` 0-1; `mods`), `gamepad` (`kind`: `connected`, `state`, `disconnected`; `index`; `id`; `buttons` as `{ pressed, value }` and `axes`, both in the W3C Gamepad API's standard mapping), `focus` (`on`) and `visible` (`on`). On the viewport's Mach channel the viewer SHALL send each event as one `input` notification whose params are that single event, one event per message, in the order they happened. `bevy-ts/viewport_input` (an array of events) SHALL accept the same events over HTTP, for tests and for clients without the channel. The surface protocol is shared with abyme's browser host, so `buttons`, `caps` and a gesture's `x`, `y` and `phase` (protocol 1.9) SHALL be accepted, and the engine MAY ignore them.

#### Scenario: Input on the channel
- **WHEN** the viewer sends a pointer `down` and then `up` as two `input` notifications
- **THEN** the engine applies them as two events in that order, and a message holding an array or another method name for input is rejected as malformed

#### Scenario: A test drives the game over HTTP
- **WHEN** a test posts `bevy-ts/viewport_input` with a `key` down and up of `KeyW`
- **THEN** a system reading `ButtonInput<KeyCode>` sees `KeyW` just pressed on one frame and just released on a later one

### Requirement: Events become the messages winit would send
`touch` SHALL become `TouchInput` (and `WindowEvent::TouchInput`) for each touch that started, moved or ended: a touch new to `touches` is `Started`, one at a new position `Moved`, and one absent that was down `Ended`, or `Canceled` on `cancel`, at its last position. The engine SHALL apply queued events at the start of each frame, before `PreUpdate`, in arrival order, as the Bevy 0.20 messages winit's backend writes for the virtual window: `CursorMoved`, `CursorEntered`, `CursorLeft`, `MouseButtonInput`, `MouseMotion`, `MouseWheel`, `PinchGesture`, `RotationGesture`, `DoubleTapGesture`, `KeyboardInput`, `Ime`, `WindowFocused`, and the matching `WindowEvent`s, and SHALL update `Window::cursor_position`. Coordinates SHALL be converted from points to the window's logical pixels. Several `move` events in one frame SHALL each produce a `CursorMoved`. Keys SHALL be released when focus is lost.

#### Scenario: A click on the cube
- **WHEN** the viewer sends `move`, `down` and `up` at the cube's position with `button: left`
- **THEN** a `bevy_picking` `PointerClick` observer on the cube fires once, from the stock mouse pointer

#### Scenario: A pinch on the phone
- **WHEN** the viewer sends `touch` `start` with touch 1, `start` with touches 1 and 2, `move` with both, and `end` with touch 2 alone
- **THEN** the game sees `TouchInput` `Started` for 1 and 2, `Moved` for each that moved, and `Ended` for 1 where it last was, on the virtual window with positions in its logical pixels

#### Scenario: Focus leaves the hole with a key held
- **WHEN** `KeyW` is down and the viewer sends `focus` with `on: false`
- **THEN** the game sees `KeyW` released and `WindowFocused { focused: false }`

### Requirement: Gamepads are read in the engine, gated by focus
Gamepads on the engine's machine SHALL be read by the engine process itself (`gilrs`), not forwarded. A gamepad on the viewer's device (a phone's) SHALL arrive as `gamepad` input and be written as the raw events gilrs writes: an entity per `index`, connected on `connected` (or a first `state`) and disconnected on `disconnected`, and each button or axis whose value changed mapped from the W3C standard mapping to `GamepadButton` and `GamepadAxis` (buttons 0-3 `South`, `East`, `West`, `North`; 4-7 `LeftTrigger`, `RightTrigger`, `LeftTrigger2`, `RightTrigger2`; 8-9 `Select`, `Start`; 10-11 the thumbs; 12-15 the D-pad; 16 `Mode`; axes 0-3 the sticks, y negated so up is positive; any other `Other(n)`). While the viewport does not have focus, gamepad input from either SHALL NOT reach the game unless `bevy-ts/viewport_mode` set `gamepadUnfocused: true`.

#### Scenario: A phone's controller
- **WHEN** the viewer sends `gamepad` `connected` at index 0 and, with focus, a `state` with button 0 pressed and axis 1 at -1
- **THEN** the game's `Gamepad` has `South` just pressed and `LeftStickY` at 1

#### Scenario: Typing in abyme's editor
- **WHEN** the operator types in a file in abyme while a gamepad is connected
- **THEN** the game receives neither the keys nor gamepad input until the hole has focus again

### Requirement: The engine tells the viewer about cursor and IME
The engine SHALL send the viewer, on the channel: `cursor` (`icon` as a CSS cursor name, or `hidden`) when the virtual window's `CursorIcon` or visibility changes; `cursor_grab` (`mode`: `none`, `confined`, `locked`) when `CursorOptions::grab_mode` changes; `cursor_warp` (`x`, `y`) when game code sets the cursor position; `ime_area` (`x`, `y`, `w`, `h` in points) when `Window::ime_position` changes; and `ime_allowed` (`on`) when `Window::ime_enabled` changes. While `locked`, `pointer` events SHALL still carry raw `dx`, `dy`.

#### Scenario: A first-person game locks the mouse
- **WHEN** the game sets `CursorGrabMode::Locked`
- **THEN** the viewer gets `cursor_grab` with `locked`, and mouse look keeps working from the raw deltas
