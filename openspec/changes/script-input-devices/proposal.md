## Why

A TS system can read the keyboard (`Keys`, from text-input) but not the mouse, the cursor, touch or
gamepads, and it cannot gate a system on input without paying a JS call each frame. Coverage audit
row 7 asks for all of Bevy 0.20's input from TS. Research is in
[docs/research/script-input-devices.md](../../../docs/research/script-input-devices.md).

## What Changes

- `bevy-ts/input` gains four exported params, each over Bevy's own state:
  - `MouseButtons` over `ButtonInput<MouseButton>`: `pressed`, `justPressed`, `justReleased`, as `Keys`.
  - `Cursor` over the primary `Window`: `position()` and `physicalPosition()`, `Vec2 | undefined`.
  - `Gamepads` over `Query<&Gamepad>`: button state, `get`/`getUnclamped`, sticks, d-pad, vendor and
    product ids, each taking the pad's entity first.
  - `Touches` over `Res<Touches>`: Bevy's `Touches` methods, `Touch` as a `HostRecord`, and index
    accessors in place of Bevy's iterators.
- Three generated enums, `MouseButton`, `GamepadButton` and `GamepadAxis`, named as in Bevy, in
  number ranges disjoint from `KeyCode` and from each other (0x1000, 0x2000, 0x2100).
- Four run conditions, `inputPressed`, `inputJustPressed`, `inputJustReleased` and
  `inputToggleActive`, evaluated in Rust over `ButtonInput<KeyCode>` or `ButtonInput<MouseButton>`,
  so a skipped system costs no JS call.
- Docs: the input section of `docs/native.md`, with a coverage table of every Bevy 0.20 input type,
  its TS path and its owner.
- Headless tests in `crates/bevy-ts-cli/tests/` for every param, enum and condition; later stages
  add tests for `Res<AccumulatedMouse*>` (after rust-resources) and for input messages, gestures and
  rumble (after rust-events and reflect-coverage stage 1).
- Not here: input messages (rust-events), `Res<T>` of input resources (rust-resources), condition
  combinators (ecs-schedules), writing `GamepadSettings`, rumble in viewport mode, and logical keys
  (`ButtonInput<Key>`). Each is named with its owner in the coverage table.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `script-input`: (introduced by text-input) gains the mouse, cursor, touch and gamepad params, their
  enums, the input run conditions, and the coverage table's promises for messages and resources.

## Impact

- Code: `crates/bevy-ts-cli/src/input.rs` (params, module JS and d.ts); `crates/bevy-ts/src/keys.rs`
  (the input number tables); `crates/bevy-ts/src/manifest.rs` and `systems.rs` (four
  `ConditionDecl` arms, compile and evaluate); new `crates/bevy-ts-cli/tests/input_devices.rs`;
  `docs/native.md`.
- Order: stage 1 starts after text-input lands, which owns `input.rs` until then. Stage 2 waits on
  rust-resources; stage 3 on rust-events and reflect-coverage stage 1.
- No new dependency: gilrs and every input feature are already compiled into the engine.
- No new export pattern: `Touches` uses index accessors, agreed with lead@writable-native-params.
- New convention, inside existing patterns: generated input enums take disjoint number ranges so
  one number names one device. Decided by lead@script-input-devices.
- Performance: each method is one fast op; the conditions remove JS calls.
- Ask-first items: none of its own. Stage 3's rumble rides rust-events' writer, whose
  per-message serialization cost waits on an operator OK there.
