## ADDED Requirements

### Requirement: An input intrinsic over Bevy's TextInput
`<input>` SHALL be a `<node>` with Bevy 0.20's `TextInput` (which requires `EditableText`), as `<button>` is a `<node>` with `Button`. It SHALL take the node props (`style`, `background`, `border`, `radius`), `size` and `color` (mapping to `TextFont` and `TextColor` as on `<text>`), and:
- `initialValue`: the text at mount, set through `EditableText::new` or the editor in Rust, never through reflection (`EditableText` has no `Reflect`). Like every `initial*` prop, a reload SHALL NOT reset it.
- `maxCharacters`: `EditableText::max_characters`; edits past it are ignored, as Bevy does.
- `autoFocus`: when true, the entity gets `AutoFocus`, so `InputFocus` moves to it when it is added.

The field SHALL be one line, Bevy's default (`allow_newlines: false`, one visible line). Focus, typing, selection, paste and Escape SHALL be Bevy's own `TextInput` behaviour; bevy-ts SHALL NOT capture keys for the field itself.

#### Scenario: A focused field with a starting value
- **WHEN** `<input initialValue="hi" autoFocus />` mounts
- **THEN** the entity has `Node`, `TextInput` and `EditableText` whose value is `"hi"`, and `InputFocus` holds it

#### Scenario: A reload keeps what was typed
- **WHEN** the operator types `"abc"` into a field mounted with `initialValue=""` and the file reloads
- **THEN** the field's entity id and its value `"abc"` are kept

#### Scenario: Too many characters
- **WHEN** `<input maxCharacters={3} autoFocus />` mounts and `a`, `b`, `c`, `d` are typed
- **THEN** its value is `"abc"`

### Requirement: Events from an input, batched
`<input>` SHALL take `onTextEditChange`, `onKeyboardInput`, `onSubmit` and `onCancel`. Each SHALL be an entity-local observer on the field, as the picking props are:
- `onTextEditChange` of `TextEditChange`, named `on` plus the Bevy event; the handler gets `{ entity, value }`. It fires when Bevy fires it, which includes cursor moves with no change to `value`.
- `onKeyboardInput` of `FocusedInput<KeyboardInput>` whose original target is the field, named `on` plus the Bevy event; the handler gets `{ entity, key, code, text, state, repeat, value }`: `key` the logical key as a W3C `KeyboardEvent.key` string (`"Enter"`, `"Escape"`, `"a"`), `code` the `KeyCode` as a W3C `KeyboardEvent.code` string (`"KeyA"`), `text` the key's text or `null`, `state` `"down"` or `"up"`.
- `onSubmit` from the same observer, on `Enter` down that is not a repeat; the handler gets `{ entity, value }`.
- `onCancel` from the same observer, on `Escape` down that is not a repeat; the handler gets `{ entity }`.

`onSubmit` and `onCancel` are the one place bevy-ts names an event Bevy lacks: Bevy 0.20 has no submit, and its own widgets that need one (feathers' number input, `ValueChange { is_final: true }` on Enter) build it the same way, from a keyboard observer on the field.

`value` SHALL be the field's text with this key and every earlier key applied: Bevy's global `TextInput` observer runs first and queues this key's edit, and bevy-ts applies the field's pending edits before reading `EditableText::value()` in Rust. It is sent as a string. Events SHALL be queued and delivered with picking events in the frame's one host call, in arrival order, each with a `ctx` holding `commands` and `frame`; for one key, `onKeyboardInput` comes before `onSubmit` or `onCancel`. Escape SHALL keep Bevy's behaviour: the field loses focus, and the handlers still run.

#### Scenario: Type and submit
- **WHEN** a focused `<input onKeyboardInput={k} onSubmit={s} />` in viewport mode receives key down events `h`, `i` and `Enter` through `bevy-ts/viewport_input`
- **THEN** `k` runs with `value` `"h"`, `"hi"` and `"hi"`, and `s` runs once with `value` `"hi"`

#### Scenario: Three keys in one frame
- **WHEN** `h`, `i` and `Enter` reach the field in the same frame
- **THEN** `onSubmit` still gets `value` `"hi"`

#### Scenario: Escape
- **WHEN** a focused `<input onCancel={c} onSubmit={s} />` receives `Escape`
- **THEN** `c` runs once, `s` does not, and `InputFocus` is empty afterwards

#### Scenario: A held Enter
- **WHEN** `Enter` is held and the viewer sends repeats
- **THEN** `onSubmit` runs once, and `onKeyboardInput` runs for each repeat with `repeat` true

#### Scenario: A change event
- **WHEN** `a` is typed into a focused `<input onTextEditChange={h} />`
- **THEN** `h` runs with `value` `"a"` in the frame the edit is applied
