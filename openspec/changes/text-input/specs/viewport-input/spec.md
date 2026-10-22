## ADDED Requirements

### Requirement: Typed text is sent once
The viewer SHALL send each typed character once: a plain key as a `key` event's `text` (which becomes `KeyboardInput` text), and a composition as an `ime` `commit` (which becomes `Ime::Commit`), never both for the same input. IME is on while an `<input>` has focus: Bevy 0.20's `TextInput` sets `Window::ime_enabled`, so the viewer gets `ime_allowed` with `on: true`. `TextInput` inserts both a key's `text` and an IME commit, so a viewer that sent both would type each character twice; abyme's viewer sends each once (research@viewer-ime-text, 2026-10-23). The engine SHALL apply each as it arrives and SHALL NOT drop either. A character typed with Option or AltGr held arrives as key `text` with `alt` set, and Bevy's `TextInput` inserts text only with no modifier or Shift, so it is dropped upstream; bevy-ts does not work around it.

#### Scenario: Plain typing with IME allowed
- **WHEN** a focused field has turned IME on and the viewer sends `key` down for `KeyH` with `text: "h"` and no `ime` event
- **THEN** the field's value is `"h"`

#### Scenario: A composed character
- **WHEN** the viewer sends `ime` `preedit` and then `ime` `commit` with `text: "é"`, and its `key` events carry no `text`
- **THEN** the field's value is `"é"`, once

#### Scenario: An Option character
- **WHEN** the viewer sends `key` down for `KeyE` with `text: "´"` and `mods.alt` true
- **THEN** the field's value does not change, as in Bevy

#### Scenario: Typing needs the virtual window
- **WHEN** a test needs to type into a field
- **THEN** it runs in viewport mode and posts `key` events to `bevy-ts/viewport_input`; under `--headless` there is no primary window, and Bevy dispatches keys to `InputFocus` only when one exists, so no field would receive them
