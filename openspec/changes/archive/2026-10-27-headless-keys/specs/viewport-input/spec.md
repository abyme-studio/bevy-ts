## RENAMED Requirements

- FROM: `### Requirement: A headless pointer over HTTP`
- TO: `### Requirement: Headless input over HTTP`

## MODIFIED Requirements

### Requirement: Headless input over HTTP
Under `bevy-ts dev --headless`, on every platform, `bevy-ts/viewport_input` SHALL be served, registered by `bevy-ts-viewport` as under `--viewport`, with the same params and result. It SHALL take `pointer`, `key` and `ime` events.

`pointer` events SHALL drive one Bevy custom pointer (`PointerId::Custom`) whose location targets the headless offscreen image, with `x`, `y` in that image's pixels (scale 1): `move` and `enter` move it, `down` and `up` press and release `left`, `right` or `middle`, and `leave` cancels it and takes it off the image, so it hovers nothing until it moves again. A `down` or `up` away from the pointer first moves it there, a frame before the press or release. `back` and `forward` SHALL be accepted and ignored.

`key` and `ime` events SHALL become Bevy's `KeyboardInput` and `Ime` on the headless primary window (the window-less one the headless pointer spawns), mapped by the same code as under `--viewport`, so a focused `<input>` receives them through `InputFocus` as in a viewport. `mods` SHALL be accepted; as under `--viewport`, Bevy reads modifiers from held keys, not from `mods`.

The engine SHALL apply queued events of all three types in arrival order, at most one per frame, pointer events in `bevy_picking`'s input step, so hover from a `move` is in place before a following `down`. An event of any other type SHALL refuse the whole request with an error naming that type, and queue nothing. A `viewport` other than none or 0 SHALL be refused. `bevy-ts/hello` SHALL still report `viewport: false`.

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
- **WHEN** a test posts `bevy-ts/viewport_input` with a `key` event and a `wheel` event under `--headless`
- **THEN** the call fails with an error naming `wheel`, and no event is applied

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
