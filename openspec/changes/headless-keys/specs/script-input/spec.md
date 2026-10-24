## MODIFIED Requirements

### Requirement: A built-in input module
Every engine SHALL ship the built-in module `bevy-ts/input`, registered with `ts_module`, its declarations written to `native-modules.d.ts` by `bevy-ts types`. It SHALL export two exported params (native-functions):
- `Keys`, over `Res<ButtonInput<KeyCode>>`, with `pressed(code)`, `justPressed(code)` and `justReleased(code)`, each returning `boolean`.
- `InputFocus`, over `Res<InputFocus>`, with `get()` returning the focused `Entity`, or `undefined` while nothing has focus, and `typing()` returning `true` when the focused entity has `EditableText` (a text field), else `false`.

They SHALL read Bevy's own resources, so they agree with Rust systems in the same frame, and SHALL NOT add a resource or a system of their own.

`get()` is not a shortcut guard: in viewport mode and headless there is a primary window, and Bevy's `set_initial_focus` puts `InputFocus` on it, so `get()` is the window while no field has focus. A single-key shortcut SHALL be written `keys.justPressed(KeyCode.KeyN) && !focus.typing()`.

#### Scenario: A key read while nothing has focus
- **WHEN** a system taking `keys: Keys, focus: InputFocus` runs `if (keys.justPressed(KeyCode.KeyN) && !focus.typing()) count += 1`, and `N` is pressed in viewport mode with `InputFocus` on the primary window
- **THEN** `count` is 1 after that frame and stays 1 on the next, and `focus.get()` is the window

#### Scenario: A key typed into a field
- **WHEN** a field has focus and `N` is pressed
- **THEN** `keys.justPressed(KeyCode.KeyN)` is still true, as in Bevy, `focus.get()` is the field, `focus.typing()` is true, and `count` stays 0

#### Scenario: After Escape
- **WHEN** a focused field receives `Escape` and `N` is pressed on a later frame
- **THEN** `focus.get()` is `undefined`, `focus.typing()` is false and `count` is 1

#### Scenario: Headless
- **WHEN** the same system runs under `--headless` and a test posts `N`, first with focus on the window and then after clicking an `<input>`
- **THEN** `count` is 1 after the first and stays 1 after the second
