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
- `onSubmit`, `onCancel` or any event Bevy lacks. Bevy lists form submission as missing (`editing.rs:67`); bevy-ts does not invent it.
- A controlled `value` prop. Writing the text from TS each change would fight the editor's cursor and IME state. A script that must clear a field remounts it with a new `key`. A later change may add a write path if Bevy grows one.
- Multi-line fields, `allow_newlines`, `visible_lines`, filters, placeholder text.
- A general native-resource read through `res()`.
- Typing under `--headless`.

## Decisions

1. `<textInput>` is a new `UiKind` in `ui.rs`, a `<node>` plus `TextInput`, as `Button` is. Its props are the node props, `size`, `color` (a field draws text, so it takes `<text>`'s two text props) and the three of its own. Alternative: `<node><TextInput/></node>` with reflected components. Rejected: no `Reflect` on `EditableText`, so no value in or out.
2. `initialValue` and `maxCharacters` are applied in Rust on insert (`EditableText::new(value)`, then `max_characters`). `initialValue` follows the `initial*` rule: a reload does not rewrite it. `maxCharacters` is written when it changes.
3. The two events live beside the picking observers, in one module that owns entity-local observer props (`picking.rs` grows, or becomes `events.rs`). The prop name rule stays `on` + Bevy event. `onKeyboardInput` only takes events whose `original_event_target()` is the field, as picking does.
4. `value` with keys before it applied. Keys reach the observer in `PreUpdate`, but edits apply later in `apply_text_edits`, so three keys in one frame would see `value` without the first two. The observer applies the field's `pending_edits` first, with the same resources `apply_text_edits` uses, then reads `value()`. Bevy's editor stays the only owner of the text. Alternative: fill `value` at delivery, after the frame's edits. Rejected: a key typed after Enter in the same frame would land in Enter's `value`.
5. Payloads use W3C strings, as the viewport protocol does: `key` from `Key` (`Character(c)` is `c`; named keys by name), `code` from `KeyCode` as `KeyboardEvent.code`. The W3C-to-Bevy table already exists in `crates/bevy-ts-viewport/src/keys.rs`, generated from bevy_winit's converters with aliases where W3C and winit differ. `bevy-ts-tsx` does not depend on the viewport crate, so the table moves to the `bevy-ts` crate (`bevy_ts::keys`) and gains the Bevy-to-W3C direction there: one owner for both directions, used by the viewport, the TSX events and `bevy-ts/input`.
6. `bevy-ts/input` is a module in `crates/bevy-ts-cli` (`src/input.rs`, with `ts/input.js` and `ts/input.d.ts`), added by the engine's plugin set so every engine and project engine has it. `bevy-ts-native` moves to `[dependencies]` for `#[export]`. Alternatives: a new crate `bevy-ts-input` (one more crate for two params), or `bevy-ts-tsx` (keys are not TSX).
7. Two params, not one: `Keys` and `InputFocus`, so a system reads as Bevy's `keys: Res<ButtonInput<KeyCode>>, focus: Res<InputFocus>`. `ButtonInput` is generic and TS params are not, so the key param is named `Keys`. `focus.get()` returns `undefined` for none, as every exported `Option` does.
8. `KeyCode` as numbers: exported methods take no strings. The variant list lives once, in `bevy_ts::keys` beside the W3C table; the TS enum in `ts/input.d.ts` and `ts/input.js` is generated from it, and a test walks `KeyCode`'s reflected variants and fails when the list, the TS enum or Bevy differ. Data-carrying `Unidentified(NativeKeyCode)` is left out.
9. IME is a viewer contract, not an engine filter. The engine applies what it is sent; the viewport-input spec says the viewer sends text once. Deduplicating in the engine would guess which of two events is real and break a viewer that is correct.

## Risks / Trade-offs

- [abyme's viewer sends both key `text` and `ime` `commit`] → characters type twice in abyme. The engine tests cover both single paths; whether abyme's viewer complies is checked in abyme (the lead tells abyme's side), not here.
- [Applying `pending_edits` in the observer] needs the font and layout resources `apply_text_edits` takes. → If that proves heavy, the observer runs only for fields with an `onKeyboardInput` handler, and only when `pending_edits` is not empty.
- [Bevy upgrades rename `KeyCode` variants] → the drift test fails at the upgrade.
- [`ButtonInput` records keys typed into a field] → that is Bevy's behaviour; docs show the focus check.

## Migration Plan

Additive. No protocol schema change expected; if `key.text` or `ime` descriptions in `protocol/bevy-ts.openrpc.json` are edited, bump to the next free minor at landing and regenerate.

## Open Questions

None blocking. Whether abyme's viewer sends both IME commit and key text is abyme's to check.
