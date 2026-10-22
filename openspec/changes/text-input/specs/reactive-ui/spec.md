## ADDED Requirements

### Requirement: A text input intrinsic over Bevy's TextInput
`<textInput>` SHALL be a `<node>` with Bevy 0.20's `TextInput` (which requires `EditableText`), as `<button>` is a `<node>` with `Button`. It SHALL take the node props (`style`, `background`, `border`, `radius`), `size` and `color` (mapping to `TextFont` and `TextColor` as on `<text>`), and:
- `initialValue`: the text at mount, set through `EditableText::new` or the editor in Rust, never through reflection (`EditableText` has no `Reflect`). Like every `initial*` prop, a reload SHALL NOT reset it.
- `maxCharacters`: `EditableText::max_characters`; edits past it are ignored, as Bevy does.
- `autoFocus`: when true, the entity gets `AutoFocus`, so `InputFocus` moves to it when it is added.

The field SHALL be one line, Bevy's default (`allow_newlines: false`, one visible line). Focus, typing, selection, paste and Escape SHALL be Bevy's own `TextInput` behaviour; bevy-ts SHALL NOT capture keys for the field itself.

#### Scenario: A focused field with a starting value
- **WHEN** `<textInput initialValue="hi" autoFocus />` mounts
- **THEN** the entity has `Node`, `TextInput` and `EditableText` whose value is `"hi"`, and `InputFocus` holds it

#### Scenario: A reload keeps what was typed
- **WHEN** the operator types `"abc"` into a field mounted with `initialValue=""` and the file reloads
- **THEN** the field's entity id and its value `"abc"` are kept

#### Scenario: Too many characters
- **WHEN** `<textInput maxCharacters={3} autoFocus />` mounts and `a`, `b`, `c`, `d` are typed
- **THEN** its value is `"abc"`

### Requirement: Events from a text input, batched
`<textInput>` SHALL take `onTextEditChange` and `onKeyboardInput`, named `on` plus the Bevy event, as the picking props are. Each SHALL be an entity-local observer:
- `onTextEditChange` of `TextEditChange` on the field; the handler gets `{ entity, value }`. It fires when Bevy fires it, which includes cursor moves with no change to `value`.
- `onKeyboardInput` of `FocusedInput<KeyboardInput>` whose original target is the field; the handler gets `{ entity, key, code, text, state, repeat, value }`: `key` the logical key as a W3C `KeyboardEvent.key` string (`"Enter"`, `"Escape"`, `"a"`), `code` the `KeyCode` as a W3C `KeyboardEvent.code` string (`"KeyA"`), `text` the key's text or `null`, `state` `"down"` or `"up"`. `value` SHALL be the field's text with every key that reached it before this one applied.

`value` SHALL be read from `EditableText` in Rust and sent as a string. Events SHALL be queued and delivered with picking events in the frame's one host call, in arrival order, each with a `ctx` holding `commands` and `frame`. There SHALL be no built-in submit or cancel event: a script submits on `key === "Enter"` and cancels on `key === "Escape"`, as Bevy's examples do. Escape SHALL keep Bevy's behaviour: the field loses focus, and the handler still runs.

#### Scenario: Type and submit
- **WHEN** a focused `<textInput onKeyboardInput={h} />` in viewport mode receives key down events `h`, `i` and `Enter` through `bevy-ts/viewport_input`
- **THEN** `h` runs for each, and the call for `Enter` has `key` `"Enter"` and `value` `"hi"`

#### Scenario: Three keys in one frame
- **WHEN** `h`, `i` and `Enter` reach the field in the same frame
- **THEN** the `Enter` call still has `value` `"hi"`

#### Scenario: Escape
- **WHEN** a focused field receives `Escape`
- **THEN** its `onKeyboardInput` runs with `key` `"Escape"`, and `InputFocus` is empty afterwards

#### Scenario: A change event
- **WHEN** `a` is typed into a focused `<textInput onTextEditChange={h} />`
- **THEN** `h` runs with `value` `"a"` in the frame the edit is applied
