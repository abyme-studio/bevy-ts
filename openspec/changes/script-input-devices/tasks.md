## 1. Stage 1: params, enums, conditions (after text-input lands; do not touch input.rs before; conditions after ecs-schedules stage 1)

- [x] 1.1 Rebase on main with text-input landed; confirm `KeysParam`, `KEY_CODES` and `bevy-ts/input` as specced
- [x] 1.2 Add `MOUSE_BUTTONS` (0x1000), `GAMEPAD_BUTTONS` (0x2000) and `GAMEPAD_AXES` (0x2100) beside `KEY_CODES` in `crates/bevy-ts/src/keys.rs`, with decoders; test list = Bevy variants and no range overlap
- [x] 1.3 Generate `MouseButton`, `GamepadButton` and `GamepadAxis` into the module JS and d.ts in `crates/bevy-ts-cli/src/input.rs`; extend the enum test
- [x] 1.4 `MouseButtons` param over `Option<Res<ButtonInput<MouseButton>>>`: `pressed`, `justPressed`, `justReleased`
- [x] 1.5 `Cursor` param over `Query<&Window, With<PrimaryWindow>>`: `position`, `physicalPosition`
- [x] 1.6 `Gamepads` param over `Query<&Gamepad>`: button state, `get`, `getUnclamped`, sticks, `dpad`, `vendorId`, `productId`; non-pad entity throws
- [x] 1.7 `Touch` `HostRecord` and `Touches` param: Bevy's methods plus `count`/`at` and the `justPressed`/`justReleased`/`justCanceled` index accessors
- [x] 1.8 `ts_condition`s `input:pressed`, `input:just-pressed`, `input:just-released`, `input:toggle-active` in `ScriptInputPlugin`, each Bevy's own condition over `KeyCode` or `MouseButton`; gamepad numbers fail the load (after ecs-schedules stage 1)
- [x] 1.9 Typed wrappers and d.ts for `inputPressed`, `inputJustPressed`, `inputJustReleased`, `inputToggleActive` over `nativeCondition` in `bevy-ts/input`
- [x] 1.10 `crates/bevy-ts-cli/tests/it/input_devices.rs`: one headless test per param, the bad-number and non-pad errors, each condition with a no-JS-call check, and the toggle sequence
- [x] 1.11 `docs/native.md` input section: the params, enums and conditions, an example, and the full coverage table (type, TS path, owner)
- [x] 1.12 `native-modules.d.ts` is generated per project by `bevy-ts types`, from the module's d.ts (no checked-in copy); land's batch check is the gate

## 2. Stage 2: mouse motion and scroll (after rust-resources lands `Res<T>` of Rust resources)

- [x] 2.1 Test `Res<AccumulatedMouseMotion>` and `Res<AccumulatedMouseScroll>` from TS, fed by `MouseMotion` / `MouseWheel`; `MouseScrollPixelsPerLine` is unreachable (Bevy 0.20 reflects it without `reflect(Resource)`)
- [x] 2.2 Document them in the input section; mark their table rows done

## 3. Stage 3: messages, gestures and rumble (after rust-events and reflect-coverage stage 1)

- [x] 3.1 Test `MessageReader` of `MouseButtonInput`, `CursorMoved`, `TouchInput`, `PinchGesture` and `GamepadEvent` from TS: rust-events' `input_messages_reach_a_ts_reader` and `window_messages_reach_a_ts_reader` own it (`tests/it/rust_events.rs`), so no second test
- [x] 3.2 Test `MessageWriter<GamepadRumbleRequest>`: drain `Messages<GamepadRumbleRequest>` and check pad, duration and intensity: rust-events' `a_ts_writer_sends_a_rumble_request_a_rust_reader_sees` owns it
- [x] 3.3 Document input messages, gestures and rumble, with `Duration` as rust-events spells it; mark their table rows done (docs/native.md; its example type-checked by `packages/bevy-ts/test/input_messages.ts`)
- [ ] 3.4 Run the repo gate
