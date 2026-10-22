## Why

A script can put Bevy's `TextInput` on a `<node>` and type in it, but it cannot set a starting value, read what was typed, or hear Enter or Escape: `EditableText` has no `Reflect`, and picking is the only event family TSX props reach. A script also cannot read one key: `res()` takes only TS resources and `Time`, so `ButtonInput<KeyCode>` is out of reach. agent town (spec@slice-3) needs, and the operator ruled (2026-10-23) that game UI is bevy_ui through TSX with an `<input>` on `bevy_ui_widgets::TextInput`: a one-line field that submits on Enter, closes on Escape, is focused on open, can be typed into by a test, and a single-key read while nothing has focus.

## What Changes

- New UI intrinsic `<input>`: a `<node>` with Bevy 0.20's `TextInput` (which brings `EditableText`), as `<button>` is a `<node>` with `Button`. Props: `initialValue`, `maxCharacters`, `autoFocus` (inserts `AutoFocus`), `size` and `color` (as `<text>`), and the node props. One line, Bevy's default.
- Four event props, entity-local observers as the picking props are:
  - `onTextEditChange({ entity, value })` from `TextEditChange`.
  - `onKeyboardInput({ entity, key, code, text, state, repeat, value })` from `FocusedInput<KeyboardInput>` on the field.
  - `onSubmit({ entity, value })` on Enter and `onCancel({ entity })` on Escape, from the same observer. Bevy has no submit event; feathers' number input builds `ValueChange { is_final: true }` on Enter the same way (operator decision, 2026-10-23).
- New built-in module `bevy-ts/input`: exported params `Keys` (over `Res<ButtonInput<KeyCode>>`) and `InputFocus` (over `Res<InputFocus>`), and a generated `KeyCode` enum. A system writes `keys.justPressed(KeyCode.KeyN) && focus.get() === undefined`.
- IME is on while a field has focus (`TextInput` turns it on). The viewer sends typed text once: a plain key as `key` `text`, a composition as `ime` `commit`, never both, since `TextInput` inserts both. abyme's viewer already does. Option and AltGr characters are dropped by Bevy's `TextInput`.
- Docs say typing reaches a field only in viewport mode; `--headless` has no primary window, so Bevy routes no key to a field.

## Capabilities

### New Capabilities
- `script-input`: the `bevy-ts/input` module: `Keys`, `InputFocus` and `KeyCode` for TS systems.

### Modified Capabilities
- `reactive-ui`: the `<input>` intrinsic and its `onTextEditChange`, `onKeyboardInput`, `onSubmit` and `onCancel` events.
- `jsx-runtime`: `<input>` joins the intrinsics.
- `viewport-input`: typed text is sent once, as key text or as an IME commit.

## Impact

- `crates/bevy-ts-tsx`: `ui.rs` (the intrinsic and its props), `picking.rs` or a sibling for the two new observers (the keyboard one also raises `onSubmit` and `onCancel`), the value read from `EditableText` in Rust.
- `crates/bevy-ts`: `keys.rs` moves here from `crates/bevy-ts-viewport`, both directions (W3C to Bevy and back) and the `KeyCode` list.
- `crates/bevy-ts-cli`: a `bevy-ts/input` module (`#[export]` params and its JS and `.d.ts`), added to every engine; `bevy-ts-native` moves from a dev-dependency to a dependency. Both are workspace crates.
- `packages/bevy-ts`: `jsx-runtime.d.ts` and `jsx-runtime.js` (props, event payloads), `native-modules.d.ts` gains `bevy-ts/input`.
- Tests in viewport mode through `bevy-ts/viewport_input`.
- Docs: `docs/ui.md` (new, the UI intrinsics and their events), `docs/native.md` (the `bevy-ts/input` built-in module), `docs/viewport.md` (typing needs viewport mode).
- Protocol: no schema change expected. If the `key.text` or `ime` descriptions are edited, the next free minor at landing.
- Operator go: none needed. No AI prompt, tool or agent loop; no new external dependency or service; no data or production surface. The two observers run only on fields that have handlers, and `Keys` and `InputFocus` cost one host call each, only when a system calls them.
