## Why

Under `bevy-ts dev --headless`, `bevy-ts/viewport_input` refuses `wheel` and `gesture`, so a headless test cannot scroll, zoom or pinch. And a headless pointer move leaves the primary window's cursor unset, so `Cursor.position()` reads `undefined` headless where a window reads the cursor.

## What Changes

- Headless `bevy-ts/viewport_input` takes `wheel` and `gesture` events. They become `MouseWheel`, `PinchGesture`, `RotationGesture` and `DoubleTapGesture` on the headless primary window, by the same code as under `--viewport`. A `wheel` also scrolls the headless pointer (`Pointer<Scroll>`); one with `x`, `y` away from it moves it there first, a frame before, as a `down` does.
- Every headless `pointer` event also goes to the primary window as under `--viewport`: `CursorMoved`, `CursorEntered`, `CursorLeft`, `MouseButtonInput`, `MouseMotion`. The window's cursor follows the pointer, so `Cursor.position()` reads the same headless and in a window, and `leave` clears it.
- The pointer, wheel and gesture mapping moves out of the macOS-only `input.rs` into `mouse.rs`, built on every platform, one owner for both paths, as `keyboard.rs` is for keys.
- `touch`, `gamepad`, `focus` and `visible` are still refused headless, naming the type.

## Capabilities

### Modified Capabilities
- `viewport-input`: the headless requirement takes `wheel` and `gesture`, and pointer events set the window's cursor.
- `shared-surface`: its windowed-mode requirement names the headless event types.

## Impact

- `crates/bevy-ts-viewport/src/mouse.rs` (new, every platform), `input.rs` (calls it), `offscreen.rs` (accepts and applies the new types), `lib.rs`.
- `crates/bevy-ts-viewport/tests/it/headless.rs` (new): a wheel, gestures, the window's cursor after a move and a leave, a refused `touch`.
- `crates/bevy-ts-cli/tests/it/headless_pointer.rs`: the refused-type case uses `touch`.
- Docs: `docs/picking.md` (Clicking headless), `docs/viewport.md`.
- Protocol: none; the event types are in the schema. No dependency, no new pattern, no cost when nothing is posted.
