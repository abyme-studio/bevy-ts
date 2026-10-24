## 1. One mapping

- [ ] 1.1 `crates/bevy-ts-viewport/src/keyboard.rs` (every platform): `key` and `ime` from `input.rs::apply_one`, writing `KeyboardInput` / `Ime` and their `WindowEvent`s
- [ ] 1.2 `input.rs::apply_one` calls `keyboard.rs`; the viewport tests stay green

## 2. Headless queue

- [ ] 2.1 `offscreen.rs`: accept `key` and `ime`; `Pending` holds `ViewportInput`; refuse any other type naming it, before queueing
- [ ] 2.2 `apply`: one event a frame in arrival order; key and IME onto the `PrimaryWindow` through `keyboard.rs`
- [ ] 2.3 Rename to `OffscreenInputPlugin`; update `crates/bevy-ts-cli/src/cli.rs`; the window comment names keys as a second reason
- [ ] 2.4 `crates/bevy-ts-cli/src/input.rs`: `FocusParam` gains `Query<(), With<EditableText>>`; `InputFocus.typing()` is true when the focused entity has `EditableText`

## 3. Tests

- [ ] 3.1 `crates/bevy-ts-cli/tests/it/`: headless, `<input autoFocus onSubmit>`, `h`, `i`, `Enter` down and up: `{ queued: 6 }`, `onSubmit` once with `"hi"`
- [ ] 3.2 Headless click on an `<input>` without `autoFocus`, then `a`: value `"a"`
- [ ] 3.3 `headless_pointer.rs`: the refused-type case uses `wheel`, with a `key` in the same request, and nothing applies
- [ ] 3.4 `headless_ui.rs` still passes (default UI camera)
- [ ] 3.5 `InputFocus.typing()`: `N` with focus on the primary window counts, `N` with focus in an `<input>` does not, headless and in viewport mode

## 4. Docs

- [ ] 4.1 `docs/viewport.md`: the headless line and Typing say keys work headless; a headless typing request
- [ ] 4.2 `docs/picking.md` Clicking headless: pointer, key and IME, one queue, one event a frame
- [ ] 4.3 `docs/ui.md`: keys reach a field in a window, in viewport mode and headless; the shortcut guard is `!focus.typing()`
- [ ] 4.5 `docs/native.md`: the `bevy-ts/input` example is `keys.justPressed(KeyCode.KeyN) && !focus.typing()`, with why `get()` is the window in viewport mode and headless
- [ ] 4.4 `openspec validate headless-keys --strict` passes
