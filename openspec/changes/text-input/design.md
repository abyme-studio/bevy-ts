## Context

Research (text-input/research, 2026-10-22) read the source; the facts this design rests on:

- `TextInput` requires `EditableText`, which requires `Node` (bevy_ui_widgets-0.20.0 `text_input.rs:71`, bevy_text-0.20.0 `editing.rs`). `DefaultPlugins` adds `TextInputPlugin`, `InputFocus` and `InputDispatch`. `bevy-ts-tsx` already enables `bevy_ui_widgets`.
- `EditableText` derives only `Component` and `Clone`; no `Reflect`. Its text is `value()`, its queue `pending_edits` (public), its limit `max_characters`.
- `TextEditChange { entity }` is an `EntityEvent` fired after `apply_text_edits`, also on cursor moves.
- `dispatch_focused_input` sends each `KeyboardInput` as `FocusedInput` to the focused entity, bubbling, and does nothing with no `PrimaryWindow`. Typed characters stop bubbling; Enter in one-line mode and Escape keep bubbling. Escape clears `InputFocus` first.
- `TextInput` inserts a key's `text` and also an `Ime::Commit`'s value (`text_input.rs:249`, `:547`).
- Picking props are entity-local observers that queue `(handler id, json)` on `TsxTree.reactive.events` (`crates/bevy-ts-tsx/src/picking.rs`), delivered in the frame's one host call (`packages/bevy-ts/runtime/jsx-runtime.js:825`).
- Exported params (`#[bevy_ts_native::export]`, `ts_param`, `ts_module`) are how a built-in module reaches Rust (`docs/native.md`). `res()` takes only TS resources and `Time`.
- `--headless` disables winit and sets `primary_window: None` (`crates/bevy-ts-cli/src/headless.rs:20`). Viewport mode spawns a virtual primary window, and `bevy-ts/viewport_input` key events become `KeyboardInput` on it.

## Goals / Non-Goals

**Goals:**
- A one-line field a script mounts, focuses, types into, and reads on each key, with Bevy's names.
- A single-key read gated on focus, in Bevy's idiom.
- Tests that type into a field through `bevy-ts/viewport_input`.

**Non-Goals:**
- A controlled `value` prop. Writing the text from TS each change would fight the editor's cursor and IME state. A script that must clear a field remounts it with a new `key`. A later change may add a write path if Bevy grows one.
- Multi-line fields, `allow_newlines`, `visible_lines`, filters, placeholder text.
- A general native-resource read through `res()`.
- Typing under `--headless`.

## Decisions

1. `<input>` is a new `UiKind` in `ui.rs`, a `<node>` plus `TextInput`, as `Button` is. Its props are the node props, `size`, `color` (a field draws text, so it takes `<text>`'s two text props) and the three of its own. Alternative: `<node><TextInput/></node>` with reflected components. Rejected: no `Reflect` on `EditableText`, so no value in or out.
2. `initialValue` and `maxCharacters` are applied in Rust on insert (`EditableText::new(value)`, then `max_characters`). `initialValue` follows the `initial*` rule: a reload does not rewrite it. `maxCharacters` is written when it changes.
3. The events live beside the picking observers, in one module that owns entity-local observer props (`picking.rs` grows, or becomes `events.rs`). `onTextEditChange` and `onKeyboardInput` keep the rule `on` + Bevy event. One `FocusedInput<KeyboardInput>` observer per field queues `onKeyboardInput` and then, on a non-repeat key down, `onSubmit` for Enter or `onCancel` for Escape. It only takes events whose `original_event_target()` is the field, as picking does.
4. `onSubmit` and `onCancel` are named, though Bevy has no submit (`editing.rs:67` lists form submission as missing). The operator ruled for a submit event (2026-10-23). Feathers' number input is the precedent: its keyboard observer on the field turns Enter into `ValueChange { is_final: true }`. Alternative: scripts check `key === "Enter"` in `onKeyboardInput`. It still works; `onSubmit` saves every form the same three lines. A held Enter submits once: repeats are skipped.
5. `value` is the text with this key and every earlier key applied. Bevy runs global observers before entity observers, so `TextInput`'s observer has already queued this key's edit when ours runs. Edits apply later in `apply_text_edits`, so ours applies the field's `pending_edits` first, with the resources `apply_text_edits` uses, then reads `value()`. Bevy's editor stays the only owner of the text. Alternative: fill `value` at delivery, after the frame's edits. Rejected: a key typed after Enter in the same frame would land in the submitted value.
6. Payloads use W3C strings, as the viewport protocol does: `key` from `Key` (`Character(c)` is `c`; named keys by name), `code` from `KeyCode` as `KeyboardEvent.code`. The W3C-to-Bevy table already exists in `crates/bevy-ts-viewport/src/keys.rs`, generated from bevy_winit's converters with aliases where W3C and winit differ. `bevy-ts-tsx` does not depend on the viewport crate, so the table moves to the `bevy-ts` crate (`bevy_ts::keys`) and gains the Bevy-to-W3C direction there: one owner for both directions, used by the viewport, the TSX events and `bevy-ts/input`.
7. `bevy-ts/input` is a module in `crates/bevy-ts-cli` (`src/input.rs`, with `ts/input.js` and `ts/input.d.ts`), added by the engine's plugin set so every engine and project engine has it. `bevy-ts-native` moves to `[dependencies]` for `#[export]`. Alternatives: a new crate `bevy-ts-input` (one more crate for two params), or `bevy-ts-tsx` (keys are not TSX).
8. Two params, not one: `Keys` and `InputFocus`, so a system reads as Bevy's `keys: Res<ButtonInput<KeyCode>>, focus: Res<InputFocus>`. `ButtonInput` is generic and TS params are not, so the key param is named `Keys`. `focus.get()` returns `undefined` for none, as every exported `Option` does.
9. `KeyCode` as numbers: exported methods take no strings. The variant list lives once, in `bevy_ts::keys` beside the W3C table; the TS enum in `ts/input.d.ts` and `ts/input.js` is generated from it, and a test walks `KeyCode`'s reflected variants and fails when the list, the TS enum or Bevy differ. Data-carrying `Unidentified(NativeKeyCode)` is left out.
10. IME is on while a field has focus: `TextInput` sets `Window::ime_enabled`, and the viewport sends `ime_allowed` to the viewer. Sending text once is a viewer contract, not an engine filter. abyme's viewer sends a plain key as `KeyboardInput` text and a composition as `Ime::Commit`, each once (research@viewer-ime-text). Deduplicating in the engine would guess which of two events is real and break a viewer that is correct. Option and AltGr characters are dropped by `TextInput`, which inserts only with no modifier or Shift; that is upstream behaviour and bevy-ts keeps it.

## Risks / Trade-offs

- [A viewer that sends both key `text` and `ime` `commit`] → characters type twice. abyme's viewer does not; the engine tests cover both single paths.
- [Option and AltGr characters are dropped] → upstream `TextInput` behaviour; documented, not worked around.
- [Applying `pending_edits` in the observer] needs the font and layout resources `apply_text_edits` takes. → If that proves heavy, the observer runs only for fields with a keyboard, submit or cancel handler, and only when `pending_edits` is not empty.
- [Bevy upgrades rename `KeyCode` variants] → the drift test fails at the upgrade.
- [`ButtonInput` records keys typed into a field] → that is Bevy's behaviour; docs show the focus check.

## Migration Plan

Additive. No protocol schema change expected; if `key.text` or `ime` descriptions in `protocol/bevy-ts.openrpc.json` are edited, bump to the next free minor at landing and regenerate.

## Open Questions

None.
