## ADDED Requirements

### Requirement: Typed text is sent once
The viewer SHALL send each typed character once: as a `key` event's `text` while no IME composition is under way, or as an `ime` `commit`, never both for the same input. Bevy 0.20's `TextInput` turns IME on (`Window::ime_enabled`, so the viewer gets `ime_allowed` with `on: true`) and inserts both a key's `text` and an IME commit, so a viewer that sent both would type each character twice. The engine SHALL apply each as it arrives and SHALL NOT drop either.

#### Scenario: Plain typing with IME allowed
- **WHEN** a focused field has turned IME on and the viewer sends `key` down for `KeyH` with `text: "h"` and no `ime` event
- **THEN** the field's value is `"h"`

#### Scenario: A composed character
- **WHEN** the viewer sends `ime` `preedit` and then `ime` `commit` with `text: "é"`, and its `key` events carry no `text`
- **THEN** the field's value is `"é"`, once

#### Scenario: Typing needs the virtual window
- **WHEN** a test needs to type into a field
- **THEN** it runs in viewport mode and posts `key` events to `bevy-ts/viewport_input`; under `--headless` there is no primary window, and Bevy dispatches keys to `InputFocus` only when one exists, so no field would receive them
