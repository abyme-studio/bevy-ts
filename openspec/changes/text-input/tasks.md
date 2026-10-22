## 1. Keys, one owner

- [ ] 1.1 Move `crates/bevy-ts-viewport/src/keys.rs` to `crates/bevy-ts/src/keys.rs` (`bevy_ts::keys`); the viewport uses it from there
- [ ] 1.2 Add Bevy-to-W3C: `code_name(KeyCode) -> &str` and `key_name(&Key) -> String`; round-trip test over every mapped key
- [ ] 1.3 Add the `KeyCode` list (number per data-free variant) and a test that walks `KeyCode`'s reflected variants and fails on drift

## 2. The `<input>` intrinsic

- [ ] 2.1 `crates/bevy-ts-tsx/src/ui.rs`: `UiKind::Input` for `input`; inserts `TextInput`; props: node props, `size`, `color`, `initialValue` (on insert only), `maxCharacters`, `autoFocus` (inserts `AutoFocus`)
- [ ] 2.2 Observers for `onTextEditChange` (`TextEditChange`) and `onKeyboardInput` (`FocusedInput<KeyboardInput>`, original target only), queued on `TsxTree.reactive.events` beside picking; the keyboard observer also queues `onSubmit` on Enter and `onCancel` on Escape, non-repeat key down, after `onKeyboardInput`
- [ ] 2.3 `value` for `onKeyboardInput` and `onSubmit`: apply the field's `pending_edits` (this key included) before reading `value()` (design decision 5)
- [ ] 2.4 `packages/bevy-ts/runtime/jsx-runtime.js`: deliver the four events with `ctx`; register the new props as handlers
- [ ] 2.5 `packages/bevy-ts/jsx-runtime.d.ts`: `InputProps`, `TextEditChangeEvent`, `KeyboardInputEvent`, `SubmitEvent`, `CancelEvent`, `input` in `IntrinsicElements`; a type test that `value=` and an unknown event field are errors

## 3. `bevy-ts/input`

- [ ] 3.1 `crates/bevy-ts-cli/Cargo.toml`: `bevy-ts-native` to `[dependencies]`
- [ ] 3.2 `crates/bevy-ts-cli/src/input.rs`: `#[export]` `Keys` (`pressed`, `justPressed`, `justReleased`, throwing on an unknown number) and `InputFocus` (`get`); `ts_param` both and `ts_module("bevy-ts/input", ...)` in the engine's plugin set
- [ ] 3.3 `ts/input.js` and `ts/input.d.ts`, with `KeyCode` generated from `bevy_ts::keys`; `bevy-ts types` writes them to `native-modules.d.ts`

## 4. Tests, in viewport mode through `bevy-ts/viewport_input`

- [ ] 4.1 Mount `<input autoFocus onKeyboardInput onSubmit>`; post `h`, `i`, `Enter` on separate frames: `onKeyboardInput` sees `"h"`, `"hi"`, `"hi"`; `onSubmit` runs once with `"hi"`
- [ ] 4.2 The same three keys in one post (one frame): `onSubmit` still gets `"hi"`; a held Enter with repeats submits once
- [ ] 4.3 `Escape`: `onCancel` runs once, `onSubmit` does not, and `InputFocus` is empty
- [ ] 4.4 `initialValue`, `maxCharacters`, a reload keeps typed text, `onTextEditChange` fires with the value
- [ ] 4.5 A TS system with `Keys` and `InputFocus` counts `N` only while nothing has focus: not while the field has it, again after Escape
- [ ] 4.6 IME: a `key` with `text` and no `ime` types once; an `ime` `commit` with key events carrying no `text` types once; a `key` with `alt` and `text` changes nothing
- [ ] 4.7 `keys.pressed(9999)` throws naming `Keys`, `pressed` and `9999`

## 5. Docs and specs

- [ ] 5.1 `docs/ui.md` (new): the UI intrinsics, picking props, `<input>`, its four events, IME, Option/AltGr characters dropped upstream, clearing by remounting with a new `key`; link from `README.md`
- [ ] 5.2 `docs/native.md` "Ports and built-in modules": `bevy-ts/input`, with the focus check
- [ ] 5.3 `docs/viewport.md`: typing reaches a field only in viewport mode; `--headless` has no primary window; IME is on while a field has focus; the viewer sends typed text once
- [ ] 5.4 Protocol: only if `key.text` or `ime` descriptions change, the next free minor at landing and `scripts/gen-protocol.sh`
- [ ] 5.5 `openspec validate text-input --strict` passes
