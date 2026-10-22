## 1. Stage 1: params, enums, conditions (after text-input lands; do not touch input.rs before)

- [ ] 1.1 Rebase on main with text-input landed; confirm `KeysParam`, `KEY_CODES` and `bevy-ts/input` as specced
- [ ] 1.2 Add `MOUSE_BUTTONS` (0x1000), `GAMEPAD_BUTTONS` (0x2000) and `GAMEPAD_AXES` (0x2100) beside `KEY_CODES` in `crates/bevy-ts/src/keys.rs`, with decoders; test list = Bevy variants and no range overlap
- [ ] 1.3 Generate `MouseButton`, `GamepadButton` and `GamepadAxis` into the module JS and d.ts in `crates/bevy-ts-cli/src/input.rs`; extend the enum test
- [ ] 1.4 `MouseButtons` param over `Option<Res<ButtonInput<MouseButton>>>`: `pressed`, `justPressed`, `justReleased`
- [ ] 1.5 `Cursor` param over `Query<&Window, With<PrimaryWindow>>`: `position`, `physicalPosition`
- [ ] 1.6 `Gamepads` param over `Query<&Gamepad>`: button state, `get`, `getUnclamped`, sticks, `dpad`, `vendorId`, `productId`; non-pad entity throws
- [ ] 1.7 `Touch` `HostRecord` and `Touches` param: Bevy's methods plus `count`/`at` and the `justPressed`/`justReleased`/`justCanceled` index accessors
- [ ] 1.8 `ConditionDecl` arms `InputPressed`, `InputJustPressed`, `InputJustReleased`, `InputToggleActive` in `manifest.rs`; compile and evaluate in `systems.rs`, toggle bit in `SystemMemory`; gamepad numbers fail the load
- [ ] 1.9 JS builders and d.ts for `inputPressed`, `inputJustPressed`, `inputJustReleased`, `inputToggleActive` in `bevy-ts/input`
- [ ] 1.10 `crates/bevy-ts-cli/tests/input_devices.rs`: one headless test per param, the bad-number and non-pad errors, each condition with a no-JS-call check, and the toggle sequence
- [ ] 1.11 `docs/native.md` input section: the params, enums and conditions, an example, and the full coverage table (type, TS path, owner)
- [ ] 1.12 Regenerate `native-modules.d.ts` with `bevy-ts types`; run the repo gate

## 2. Stage 2: mouse motion and scroll (after rust-resources lands `Res<T>` of Rust resources)

- [ ] 2.1 Test `Res<AccumulatedMouseMotion>`, `Res<AccumulatedMouseScroll>` and `Res<MouseScrollPixelsPerLine>` from TS, fed by `MouseMotion` / `MouseWheel`
- [ ] 2.2 Document them in the input section; mark their table rows done

## 3. Stage 3: messages, gestures and rumble (after rust-events and reflect-coverage stage 1)

- [ ] 3.1 Test `MessageReader` of `MouseButtonInput`, `CursorMoved`, `TouchInput`, `PinchGesture` and `GamepadEvent` from TS
- [ ] 3.2 Test `MessageWriter<GamepadRumbleRequest>`: drain `Messages<GamepadRumbleRequest>` and check pad, duration and intensity
- [ ] 3.3 Document input messages, gestures and rumble, with `Duration` as rust-events spells it; mark their table rows done
- [ ] 3.4 Run the repo gate
