# script-input Specification

## Purpose
`bevy-ts/input`, a built-in module every engine ships: `Keys`, `InputFocus` and `KeyCode`, so a TS system reads keys and focus from Bevy's own resources, as a Rust system does.

## Requirements

### Requirement: A built-in input module
Every engine SHALL ship the built-in module `bevy-ts/input`, registered with `ts_module`, its declarations written to `native-modules.d.ts` by `bevy-ts types`. It SHALL export two exported params (native-functions):
- `Keys`, over `Res<ButtonInput<KeyCode>>`, with `pressed(code)`, `justPressed(code)` and `justReleased(code)`, each returning `boolean`.
- `InputFocus`, over `Res<InputFocus>`, with `get()` returning the focused `Entity`, or `undefined` while nothing has focus.

They SHALL read Bevy's own resources, so they agree with Rust systems in the same frame, and SHALL NOT add a resource or a system of their own.

#### Scenario: A key read while nothing has focus
- **WHEN** a system taking `keys: Keys, focus: InputFocus` runs `if (keys.justPressed(KeyCode.KeyN) && focus.get() === undefined) count += 1`, and `N` is pressed in viewport mode with no field focused
- **THEN** `count` is 1 after that frame and stays 1 on the next

#### Scenario: A key typed into a field
- **WHEN** a field has focus and `N` is pressed
- **THEN** `keys.justPressed(KeyCode.KeyN)` is still true, as in Bevy, `focus.get()` is the field, and `count` stays 0

#### Scenario: After Escape
- **WHEN** a focused field receives `Escape` and `N` is pressed on a later frame
- **THEN** `focus.get()` is `undefined` and `count` is 1

### Requirement: KeyCode is a generated enum
`bevy-ts/input` SHALL export `KeyCode`, a TS enum with one member per Bevy 0.20 `KeyCode` variant that carries no data, named as in Rust (`KeyCode.KeyN`, `KeyCode.Enter`, `KeyCode.ArrowUp`), each a number. The TS enum and the Rust table that maps numbers back to `KeyCode` SHALL come from one generated source, and a test SHALL fail when they differ from Bevy's variants. A method given a number that is not a member SHALL throw, naming the param, the method and the number.

#### Scenario: A typo
- **WHEN** a system writes `keys.justPressed(KeyCode.KeyNN)`
- **THEN** `tsc` reports that `KeyNN` does not exist on `KeyCode`

#### Scenario: A bad number
- **WHEN** a system calls `keys.pressed(9999)`
- **THEN** the call throws naming `Keys`, `pressed` and `9999`, and the system's commands from that run are discarded

### Requirement: Input enums share one number space
`bevy-ts/input` SHALL export `MouseButton`, `GamepadButton` and `GamepadAxis`, TS enums with one member per Bevy 0.20 variant that carries no data, named as in Rust (`MouseButton.Left`, `GamepadButton.South`, `GamepadAxis.LeftStickX`); `Other(..)` variants SHALL be left out. Their numbers SHALL be disjoint from `KeyCode`'s and from each other's: `MouseButton` from `0x1000`, `GamepadButton` from `0x2000`, `GamepadAxis` from `0x2100`. Each TS enum and the Rust table that maps its numbers back SHALL come from one generated source, and a test SHALL fail when they differ from Bevy's variants or when two ranges overlap. A method or condition given a number outside the device it expects SHALL throw, naming the param or condition, the method and the number.

#### Scenario: A gamepad number to a mouse method
- **WHEN** a system calls `mouse.pressed(GamepadButton.South)`
- **THEN** the call throws naming `MouseButtons`, `pressed` and the number, and the system's commands from that run are discarded

#### Scenario: Bevy adds a variant
- **WHEN** Bevy's `GamepadButton` gains a variant the table lacks
- **THEN** the enum test fails, naming the variant

### Requirement: Mouse buttons
`bevy-ts/input` SHALL export the param `MouseButtons`, over `Res<ButtonInput<MouseButton>>`, with `pressed(button)`, `justPressed(button)` and `justReleased(button)`, each returning `boolean`. With no such resource, every method SHALL return `false`.

#### Scenario: A click
- **WHEN** Bevy receives `MouseButtonInput { button: Left, state: Pressed }` and a system runs `if (mouse.justPressed(MouseButton.Left)) clicks += 1`
- **THEN** `clicks` is 1 after that frame, stays 1 on the next, and `mouse.pressed(MouseButton.Left)` is true until the button is released

### Requirement: Cursor position
`bevy-ts/input` SHALL export the param `Cursor`, over the window with `PrimaryWindow`, with `position()` and `physicalPosition()`, each returning Bevy's `cursor_position()` / `physical_cursor_position()` as a `Vec2`, or `undefined` when the cursor is outside the window or there is no primary window.

#### Scenario: Cursor inside the window
- **WHEN** the primary window's cursor is set to `(100, 50)` in logical pixels
- **THEN** `cursor.position()` is `Vec2(100, 50)` that frame

#### Scenario: No window
- **WHEN** the app has no primary window
- **THEN** `cursor.position()` is `undefined` and nothing throws

### Requirement: Gamepads
`bevy-ts/input` SHALL export the param `Gamepads`, over `Query<&Gamepad>`. Each method SHALL take the pad's entity first: `pressed(pad, button)`, `justPressed(pad, button)` and `justReleased(pad, button)` returning `boolean`; `get(pad, input)` and `getUnclamped(pad, input)` taking a `GamepadButton` or `GamepadAxis` and returning `number | undefined`, as Bevy's `get(impl Into<GamepadInput>)`; `leftStick(pad)`, `rightStick(pad)` and `dpad(pad)` returning `Vec2`; `vendorId(pad)` and `productId(pad)` returning `number | undefined`. Pads SHALL be found as in Bevy, with `Query<[Entity], With<Gamepad>>`. An entity with no `Gamepad` SHALL throw, naming `Gamepads`, the method and the entity.

#### Scenario: A button and a stick
- **WHEN** a pad entity is connected and Bevy receives a raw `South` press of 1.0 and a `LeftStickX` value of 0.5
- **THEN** a system iterating `Query<[Entity], With<Gamepad>>` sees `gamepads.justPressed(pad, GamepadButton.South)` true that frame and `gamepads.leftStick(pad).x` equal to Bevy's processed value

#### Scenario: Not a pad
- **WHEN** a system calls `gamepads.pressed(player, GamepadButton.South)` on an entity with no `Gamepad`
- **THEN** the call throws naming `Gamepads`, `pressed` and the entity

### Requirement: Touches
`bevy-ts/input` SHALL export the param `Touches`, over `Res<Touches>`, with Bevy's `anyJustPressed()`, `anyJustReleased()`, `anyJustCanceled()`, `justPressed(id)`, `justReleased(id)`, `justCanceled(id)`, `getPressed(id)`, `getReleased(id)` and `firstPressedPosition()`. A touch SHALL be the record `Touch { id, position, startPosition, previousPosition, delta, distance }`. In place of Bevy's iterators, `count()`/`at(i)`, `justPressedCount()`/`justPressedAt(i)`, `justReleasedCount()`/`justReleasedAt(i)` and `justCanceledCount()`/`justCanceledAt(i)` SHALL list the same touches in a stable order within one run; `at(i)` out of range SHALL return `undefined`. With no such resource, the param SHALL read as no touches.

#### Scenario: A finger down and moving
- **WHEN** Bevy receives `TouchInput { phase: Started, id: 1, position: (10, 10) }`, and the next frame `Moved` to `(20, 10)`
- **THEN** the first frame `touches.anyJustPressed()` is true and `touches.justPressedAt(0).id` is 1; the next frame `touches.getPressed(1).delta` is `Vec2(10, 0)` and `touches.count()` is 1

### Requirement: Input run conditions
`bevy-ts/input` SHALL export `inputPressed(code)`, `inputJustPressed(code)`, `inputJustReleased(code)` and `inputToggleActive(default, code)`, Bevy's input conditions, taking a `KeyCode` or a `MouseButton`. They SHALL be Bevy's own conditions over `ButtonInput<KeyCode>` or `ButtonInput<MouseButton>`, chosen by the number's range, exposed as native conditions (`nativeCondition`), so they run in Rust and a skipped system costs no JS call. `inputToggleActive` SHALL keep its on/off state per use across frames and reloads, flipping it on each frame the input is just pressed, as Bevy's `Local<bool>`. A `GamepadButton` or `GamepadAxis` number SHALL fail the load at the call's location, saying gamepad buttons are read per pad with `Gamepads`.

#### Scenario: Pause on Escape
- **WHEN** `systems(pause).runIf(inputJustPressed(KeyCode.Escape))` is registered and Escape is pressed once
- **THEN** `pause` runs on that frame only, and on other frames no JS call is made for it

#### Scenario: Toggle a debug overlay
- **WHEN** `systems(overlay).runIf(inputToggleActive(false, KeyCode.F1))` is registered and F1 is pressed on frames 2 and 5
- **THEN** `overlay` runs on frames 2, 3 and 4 and not on 1, 5 or later

#### Scenario: A gamepad condition
- **WHEN** a system is registered with `runIf(inputPressed(GamepadButton.South))`
- **THEN** the load fails at the `inputPressed` call, pointing at `Gamepads`

### Requirement: Mouse motion and scroll as resources
Mouse motion and scroll SHALL be read through `Res<T>` of Bevy's own resources, with no param of `bevy-ts/input`: `Res<AccumulatedMouseMotion>` (`delta`), and `Res<AccumulatedMouseScroll>` (`unit`, `delta`), through the `Res<T>` path of Rust resources. `MouseScrollPixelsPerLine` SHALL be read with `Res<MouseScrollPixelsPerLine>` and written with `ResMut<MouseScrollPixelsPerLine>`: Bevy 0.20 reflects it without `reflect(Resource)`, so bevy-ts registers its `ReflectResource`.

#### Scenario: Mouse look
- **WHEN** Bevy receives `MouseMotion { delta: (3, -2) }` and a system reads `motion: Res<AccumulatedMouseMotion>`
- **THEN** `motion.delta` is `Vec2(3, -2)` that frame and `Vec2(0, 0)` the next

#### Scenario: Pixels per scroll line
- **WHEN** a system reads `Res<MouseScrollPixelsPerLine>` and another sets it to 40 with `ResMut<MouseScrollPixelsPerLine>`
- **THEN** the first reads Bevy's default, 100, and Bevy's `MouseScrollPixelsPerLine` is 40 after the frame

### Requirement: Input messages, gestures and rumble
Every Bevy input message SHALL be read with `MessageReader` by its Bevy type and every writable one written with `MessageWriter`, through the generic message path: mouse (`MouseButtonInput`, `MouseMotion`, `MouseWheel`), cursor (`CursorMoved`, `CursorEntered`, `CursorLeft`), touch (`TouchInput`), gestures (`PinchGesture`, `RotationGesture`, `DoubleTapGesture`, `PanGesture`) and gamepad (`GamepadEvent` and its parts, `RawGamepadEvent`). A TS system SHALL rumble a pad by writing `GamepadRumbleRequest` with `MessageWriter`, as Bevy's `gamepad_rumble` example does.

#### Scenario: A pinch
- **WHEN** Bevy receives `PinchGesture(0.25)` and a system reads `MessageReader<PinchGesture>`
- **THEN** it reads one message whose value is 0.25

#### Scenario: Rumble
- **WHEN** a system writes `GamepadRumbleRequest.Add` for a pad with a duration and an intensity
- **THEN** `Messages<GamepadRumbleRequest>` holds that request, with the same pad, duration and intensity, after the frame

### Requirement: Input coverage table
`docs/native.md` SHALL hold a table of every public Bevy 0.20 input type in `bevy_input`, the cursor parts of `bevy_window`, and `bevy_gilrs`, each with its TS path and the change or follow-up that owns it. A type with no TS path SHALL say why or name its follow-up.

#### Scenario: Looking up a type
- **WHEN** a reader looks for `GamepadSettings` in the table
- **THEN** the row says it is read through reflection and that writing it is a named follow-up

### Requirement: Focus visibility and directional navigation in bevy-ts/input
`bevy-ts/input` SHALL also export `InputFocusVisible`, over `Res<InputFocusVisible>`, with `get()` returning `boolean`, and `DirectionalNavigation`, over Bevy's `DirectionalNavigation` system param, with `navigate(direction)` taking a `CompassOctant`, a numeric TS enum like `KeyCode` (`CompassOctant.North`, `CompassOctant.NorthEast`, ..., `CompassOctant.NorthWest`) and returning the newly focused `Entity`, or `undefined` when there is no focus or no neighbour. They SHALL read and write Bevy's own resources.

#### Scenario: Arrow keys move focus
- **WHEN** a project with `"directionalNavigation": true` has a TS system that runs `if (keys.justPressed(KeyCode.ArrowDown)) nav.navigate(CompassOctant.South)` over a column of `AutoDirectionalNavigation` buttons, one focused, and `ArrowDown` is pressed in viewport mode
- **THEN** `InputFocus` holds the button below

### Requirement: A project opts into tab and directional navigation
`bevy-ts.json` SHALL accept `tabNavigation` and `directionalNavigation`, bools, default `false`, through the same path as `meshPicking`. `tabNavigation: true` SHALL add Bevy's `TabNavigationPlugin`, and `directionalNavigation: true` Bevy's `DirectionalNavigationPlugin`, under `bevy-ts dev` in a window, `--viewport` and `--headless` alike, and in a built game. When a switch is `false` or absent the engine SHALL NOT add its plugin. Neither switch SHALL be an engine hash input or start `cargo`. A value that is not a bool SHALL fail `bevy-ts dev` with a message naming the field and `bevy-ts.json`. Tab navigation SHALL work in viewport mode and in a window; under `--headless`, which has no primary window, Tab SHALL do nothing.

#### Scenario: Tab in viewport mode
- **WHEN** a project with `"tabNavigation": true` runs in viewport mode, mounts two `<button tabIndex={0}>`s in a `<node tabGroup={{}}>` and the viewer sends `Tab`
- **THEN** `InputFocus` holds the first button

#### Scenario: Off by default
- **WHEN** a project's `bevy-ts.json` has neither switch, runs in viewport mode, mounts two `<button tabIndex={0}>`s in a `<node tabGroup={{}}>` and the viewer sends `Tab`
- **THEN** `InputFocus` stays empty, and the app has no `DirectionalNavigationMap`

#### Scenario: A wrong type
- **WHEN** `bevy-ts.json` holds `"tabNavigation": "yes"`
- **THEN** `bevy-ts dev` exits 1 with a message naming `tabNavigation` and `bevy-ts.json`

#### Scenario: Built and played
- **WHEN** a project with both switches `true` is built and its folder played
- **THEN** `game.json` holds both as `true` and the played app has both plugins

#### Scenario: Play, stop, play
- **WHEN** an edit-mode engine with `"tabNavigation": true` plays, stops and plays again, then the viewer sends `Tab` once
- **THEN** focus moves one step, not two

### Requirement: Navigation used with its switch off is named
When a `tabIndex` or `tabGroup` prop mounts and `TabNavigationPlugin` was not added, the engine SHALL log one warning per process that names the entity and says to set `"tabNavigation": true` in `bevy-ts.json`; it SHALL NOT warn for a `TabIndex` Bevy writes itself (a `<tab>`), nor for the `TabIndex(0)` an `<input>` gets by default, nor when a native plugin added `TabNavigationPlugin` itself. `DirectionalNavigation.navigate` SHALL throw, naming `"directionalNavigation": true`, when the app has no `DirectionalNavigationMap`.

#### Scenario: A tab index with the switch off
- **WHEN** a game without `tabNavigation` mounts two `<button tabIndex={0}>`s
- **THEN** one warning naming the first button and `"tabNavigation": true` is logged, and no second

#### Scenario: navigate with the switch off
- **WHEN** a TS system calls `nav.navigate(CompassOctant.South)` in a game without `directionalNavigation`
- **THEN** the call throws an error naming `"directionalNavigation": true`
