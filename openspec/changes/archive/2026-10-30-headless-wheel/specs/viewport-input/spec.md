## MODIFIED Requirements

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
