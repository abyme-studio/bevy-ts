## Why

Agent town's headless walk (`test/walk-keys.mjs`) has to type into an `<input>` and submit. Under `bevy-ts dev --headless`, `bevy-ts/viewport_input` takes only `pointer` events and refuses `key`, so a headless test cannot type. The docs give the wrong reason: they say headless has no primary window. The headless pointer has spawned a window-less primary window since mesh-picking (`crates/bevy-ts-viewport/src/offscreen.rs`). What is missing is the key path.

## What Changes

- Headless `bevy-ts/viewport_input` takes `key` and `ime` events, in the same shape `--viewport` takes (`code`, `key`, `text`, `state`, `repeat`, `mods`; `kind`, `text`, `cursor`). They become Bevy's `KeyboardInput` and `Ime` on the headless primary window, so `InputFocus` dispatch, `TextInput`, `onKeyboardInput`/`onSubmit`/`onCancel`, `Keys` and `LogicalKeys` work as in a viewport.
- The event-to-message mapping for `key` and `ime` moves out of the macOS-only `input.rs` into a platform-neutral module of `bevy-ts-viewport`, one owner used by `--viewport` and `--headless`.
- Headless applies pointer, key and IME events from one queue, in arrival order, one event a frame, as it does pointers today.
- Other event types (`wheel`, `gesture`, `touch`, `gamepad`, `focus`, `visible`) are still refused, naming the type.
- Docs drop "headless has no primary window" and show a headless typing request.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `viewport-input`: the headless requirement takes `key` and `ime`, and typing works under `--headless`.

## Impact

- `crates/bevy-ts-viewport/src/keyboard.rs` (new, every platform): `key` and `ime` events to `KeyboardInput` and `Ime`, moved from `input.rs`, which calls it.
- `crates/bevy-ts-viewport/src/offscreen.rs`: accepts `key` and `ime`, one queue for all three; the plugin keeps its name or becomes `OffscreenInputPlugin` (one call site, `crates/bevy-ts-cli/src/cli.rs`).
- `crates/bevy-ts-cli/tests/it/`: a headless test types `h`, `i`, `Enter` into an `<input>` and sees `onSubmit` `"hi"`; `headless_pointer.rs`'s refused-type test moves to `wheel`.
- Docs: `docs/viewport.md` (Typing, the headless line), `docs/picking.md` (Clicking headless), `docs/ui.md`.
- Protocol: none; the event types are already in the schema.
- `headless-ui-camera` is untouched: the primary window already exists headless, and the default UI camera rule does not change.
- Separate paths from logical-keys (`keys.rs`, `input.rs` in `bevy-ts-cli`, `systems.rs`).
- Operator go: none needed. No new pattern (the existing headless queue and the existing mapping), no dependency, no performance cost when no keys are posted.
