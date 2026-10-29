# viewport-input Specification

## Purpose
Input from the viewer of a viewport (pointer, wheel, gestures, keys, IME, touch, gamepads, focus) carried on the channel or over HTTP and applied as the messages winit and gilrs would send, with cursor and IME state sent back to the viewer and gamepads also read in the engine.

## Requirements

### Requirement: Input events are protocol types carried on the channel
Viewport input SHALL be defined in `protocol/bevy-ts.openrpc.json` as `ViewportInput` events: `pointer` (`kind`: `move`, `down`, `up`, `enter`, `leave`; `x`, `y` in points from the hole's top left; `button`: `left`, `right`, `middle`, `back`, `forward`; `clicks`; `dx`, `dy` raw motion; `buttons` held; `mods`, with `shift`, `ctrl`, `alt`, `meta` and `caps`), `wheel` (`dx`, `dy`, `unit`: `line` or `pixel`, `phase`, `momentum`, `mods`), `gesture` (`kind`: `pinch`, `rotate`, `double_tap`; `delta`; `x`, `y`; `phase` as `wheel`'s), `key` (`state`: `down`, `up`; `code` as a W3C `KeyboardEvent.code`; `key` logical key; `text`; `repeat`; `mods`), `ime` (`kind`: `enabled`, `preedit`, `commit`, `disabled`; `text`; `cursor` as a UTF-8 byte range), `touch` (`kind`: `start`, `move`, `end`, `cancel`; `touches`, every touch down after the event, each `id`, `x`, `y` in points from the hole's top left, `rx`, `ry` radii in points and `force` 0-1; `mods`; `t`, when it happened in ms on the viewer device's own clock (protocol 1.20), only compared within one stream and not required to rise strictly), `gamepad` (`kind`: `connected`, `state`, `disconnected`; `index`; `id`; `buttons` as `{ pressed, value }` and `axes`, both in the W3C Gamepad API's standard mapping), `focus` (`on`) and `visible` (`on`). On the viewport's Mach channel the viewer SHALL send each event as one `input` notification whose params are that single event, one event per message, in the order they happened. `bevy-ts/viewport_input` (an array of events) SHALL accept the same events over HTTP, for tests and for clients without the channel. The surface protocol is shared with abyme's browser host, so `buttons`, `caps` and a gesture's `x`, `y` and `phase` (protocol 1.9) SHALL be accepted, and the engine MAY ignore them.

#### Scenario: Input on the channel
- **WHEN** the viewer sends a pointer `down` and then `up` as two `input` notifications
- **THEN** the engine applies them as two events in that order, and a message holding an array or another method name for input is rejected as malformed

#### Scenario: A test drives the game over HTTP
- **WHEN** a test posts `bevy-ts/viewport_input` with a `key` down and up of `KeyW`
- **THEN** a system reading `ButtonInput<KeyCode>` sees `KeyW` just pressed on one frame and just released on a later one

#### Scenario: A touch from abyme's stream carries its time
- **WHEN** the viewer sends `touch` `start` with `t: 1234.5`, and another without `t`
- **THEN** both are accepted and become `TouchInput` `Started`, and the engine MAY ignore `t`

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

### Requirement: Headless input over HTTP
Under `bevy-ts dev --headless`, on every platform, `bevy-ts/viewport_input` SHALL be served, registered by `bevy-ts-viewport` as under `--viewport`, with the same params and result. It SHALL take `pointer`, `wheel`, `gesture`, `key` and `ime` events.

`pointer` events SHALL drive one Bevy custom pointer (`PointerId::Custom`) whose location targets the headless offscreen image, with `x`, `y` in that image's pixels (scale 1): `move` and `enter` move it, `down` and `up` press and release `left`, `right` or `middle`, and `leave` cancels it and takes it off the image, so it hovers nothing until it moves again. A `down` or `up` away from the pointer first moves it there, a frame before the press or release. `back` and `forward` SHALL be accepted and press nothing on the pointer.

`wheel` events SHALL scroll the pointer where it is (`PointerAction::Scroll`, same `unit`, `x`, `y` and `phase` as the `MouseWheel`); a `wheel` with `x`, `y` away from the pointer first moves it there, a frame before, as a `down` does.

Every event SHALL also become the messages a window gets, on the headless primary window (the window-less one the headless pointer spawns), mapped by the same code as under `--viewport`: `pointer` as `CursorEntered`, `CursorMoved`, `CursorLeft`, `MouseButtonInput` and `MouseMotion`, with the window's cursor set to `x`, `y` and cleared on `leave`; `wheel` as `MouseWheel`, the cursor moved first to its `x`, `y` when it has them; `gesture` as `PinchGesture`, `RotationGesture` and `DoubleTapGesture`; `key` and `ime` as `KeyboardInput` and `Ime`. So `Cursor.position()` reads the same headless and in a window, and so a focused `<input>` receives them through `InputFocus` as in a viewport. `mods` SHALL be accepted; as under `--viewport`, Bevy reads modifiers from held keys, not from `mods`.

The engine SHALL apply queued events in arrival order, at most one per frame, pointer events in `bevy_picking`'s input step, so hover from a `move` is in place before a following `down`. An event of any other type SHALL refuse the whole request with an error naming that type, and queue nothing. A `viewport` other than none or 0 SHALL be refused. `bevy-ts/hello` SHALL still report `viewport: false`.

#### Scenario: A test clicks a mesh headless
- **WHEN** a test runs `bevy-ts dev --headless` on a project with `"meshPicking": true` and a cube with `onPointerClick`, and posts `bevy-ts/viewport_input` with `move`, `down` and `up` (`button: left`) at the cube's pixel in the offscreen image
- **THEN** the answer is `{ queued: 3 }` and the handler fires once with the cube's entity

#### Scenario: A test types and submits headless
- **WHEN** a test runs `bevy-ts dev --headless` on a project with `<input autoFocus onSubmit={s} />`, and posts `key` down and up for `KeyH` (`key: "h"`, `text: "h"`), `KeyI` (`key: "i"`, `text: "i"`) and `Enter` (`key: "Enter"`)
- **THEN** the answer is `{ queued: 6 }` and `s` runs once with `value` `"hi"`

#### Scenario: Clicking a field, then typing
- **WHEN** a headless test posts a pointer `move`, `down` and `up` on an `<input>` without `autoFocus`, then `key` events for `a`
- **THEN** the field takes focus from the click and its value is `"a"`

#### Scenario: A key headless
- **WHEN** a test posts `bevy-ts/viewport_input` with a `key` down for `KeyW` under `--headless`
- **THEN** the answer is `{ queued: 1 }`, and a system reading `ButtonInput<KeyCode>` sees `KeyW` just pressed on the frame it is applied

#### Scenario: A wheel headless
- **WHEN** a test posts a pointer `move` and a `wheel` (`dx: 1.5`, `dy: -3`, `unit: "pixel"`, `phase: "started"`, `x: 30`, `y: 40`) under `--headless`
- **THEN** the answer is `{ queued: 2 }`, the game reads one `MouseWheel` with that unit, `x`, `y` and phase on the primary window, the headless pointer scrolls the same, and the window's cursor is at 30, 40

#### Scenario: A pinch headless
- **WHEN** a test posts a `gesture` `pinch` with `delta: 0.25` under `--headless`
- **THEN** the game reads one `PinchGesture(0.25)`

#### Scenario: The cursor headless
- **WHEN** a test posts a pointer `move` to 12, 34 under `--headless`, then a `leave`
- **THEN** `Cursor.position()` is 12, 34 after the move and `undefined` after the leave

#### Scenario: A touch headless
- **WHEN** a test posts `bevy-ts/viewport_input` with a `key` event and a `touch` event under `--headless`
- **THEN** the call fails with an error naming `touch`, and no event is applied

#### Scenario: On Linux
- **WHEN** the headless click and typing tests run on Linux
- **THEN** they pass as on macOS

### Requirement: Typed text is sent once
The viewer SHALL send each typed character once: a plain key as a `key` event's `text` (which becomes `KeyboardInput` text), and a composition as an `ime` `commit` (which becomes `Ime::Commit`), never both for the same input. IME is on while an `<input>` has focus: Bevy 0.20's `TextInput` sets `Window::ime_enabled`, so the viewer gets `ime_allowed` with `on: true`. `TextInput` inserts both a key's `text` and an IME commit, so a viewer that sent both would type each character twice; abyme's viewer sends each once (research@viewer-ime-text, 2026-10-23). The engine SHALL apply each as it arrives and SHALL NOT drop either. A character typed with Option or AltGr held is dropped upstream: Bevy's `TextInput` inserts text only with no modifier or Shift, and reads modifiers from the keys held in `ButtonInput<Key>`, not from a key event's `mods`, so the viewer sends the Alt key's own `key` down first, as a native window does. bevy-ts does not work around it.

#### Scenario: Plain typing with IME allowed
- **WHEN** a focused field has turned IME on and the viewer sends `key` down for `KeyH` with `text: "h"` and no `ime` event
- **THEN** the field's value is `"h"`

#### Scenario: A composed character
- **WHEN** the viewer sends `ime` `preedit` and then `ime` `commit` with `text: "é"`, and its `key` events carry no `text`
- **THEN** the field's value is `"é"`, once

#### Scenario: An Option character
- **WHEN** the viewer sends `key` down for `AltLeft`, then `key` down for `KeyE` with `text: "´"`, both with `mods.alt` true
- **THEN** the field's value does not change, as in Bevy

#### Scenario: Typing needs the virtual window
- **WHEN** a test needs to type into a field
- **THEN** it posts `key` events to `bevy-ts/viewport_input`, in viewport mode or under `--headless`; both have a primary window, which Bevy needs to dispatch keys to `InputFocus`
