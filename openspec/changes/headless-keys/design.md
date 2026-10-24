## Context

- `dispatch_focused_input` sends each `KeyboardInput` to `InputFocus` only when one `PrimaryWindow` exists (bevy_input_focus `lib.rs:394`). `crates/bevy-ts-cli/src/headless.rs:25` sets `primary_window: None`, but `OffscreenPointerPlugin` then spawns a window-less `PrimaryWindow` the size of the offscreen image (`offscreen.rs`, `spawn_windowless_primary`), a workaround for `bevy_picking`. So headless already has the window keys need; the brief's premise and `docs/viewport.md#typing` are stale.
- Headless `viewport_input` (`offscreen.rs::input`) refuses any type but `pointer`, queues pointer events in `Pending`, and applies one a frame in `First`, `PickingSystems::Input`.
- The `key` and `ime` mapping lives in `input.rs::apply_one` (`crates/bevy-ts-viewport/src/input.rs:183-205`), a module compiled only on macOS, with `bevy_ts::keys` doing code and key names. It writes `KeyboardInput` / `Ime` and the matching `WindowEvent`s.
- `headless-ui-camera` (cb3de7a) gives `IsDefaultUiCamera` to the highest-order camera on the offscreen image. It does not look at windows.

## Goals / Non-Goals

**Goals:**
- A headless test types into an `<input>` and submits, on every platform.
- One owner for turning a `key` or `ime` event into Bevy messages.

**Non-Goals:**
- `wheel`, `gesture`, `touch`, `gamepad`, `focus` and `visible` headless.
- The `Modifiers` resource headless (the edit tools read it; headless has none).
- `ime_allowed` and other feedback to a headless caller (no channel).

## Decisions

1. Move, not copy. `key` and `ime` go to `crates/bevy-ts-viewport/src/keyboard.rs`, compiled on every platform: `pub fn key(world, window, &KeyEvent)` and `pub fn ime(world, window, ImeEvent)`, writing the message and its `WindowEvent` as `input.rs::write` does. `input.rs::apply_one` calls them. Alternative: un-gate `input.rs`. Rejected: it pulls the Mach channel, gamepads and feedback onto Linux for two arms.
2. One headless queue. `Pending` holds `ViewportInput` (pointer, key or IME), not `PointerEvent`. `apply` pops one event a frame: a pointer as today; a key or IME through `keyboard.rs` onto the `PrimaryWindow`. Arrival order holds across types, so "click the field, then type" works. Alternative: apply every key in the frame it arrives. Rejected: keys could then overtake a click queued before them. Cost: a word takes a frame per key event; `h`, `i`, `Enter` down and up are six frames, 0.1 s at 60 Hz.
3. Validation stays first: every event's `type` is checked before any is queued; an unknown type refuses the whole request, naming it.
4. The plugin becomes `OffscreenInputPlugin`, its one call site in `cli.rs` updated. It still spawns the window-less primary window; that window is now also what keys need, so the workaround comment names both reasons.
5. `headless-ui-camera` is unaffected: no window is added or removed, and its rule reads cameras on the image.

## Risks / Trade-offs

- [The headless window has `focused: false`] → `dispatch_focused_input` and `TextInput` do not read it; the test proves typing works. If a later Bevy gates on it, the plugin sets `focused: true`.
- [Bevy fixes `Location::is_in_viewport` and the window-less window is dropped] → keys would stop. The comment and the typing test guard it: drop the window only with a replacement for keys.
- [Shared crate with logical-keys] → separate files; `keys.rs` is read, not edited.

## Migration Plan

Additive. A headless caller that sent `key` and relied on the refusal now gets it applied; none is known.

## Open Questions

None.
