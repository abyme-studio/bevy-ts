## Context

text-input ships `bevy-ts/input` in `crates/bevy-ts-cli/src/input.rs`: `Keys` and `InputFocus`,
exported params over Bevy's resources, and `KeyCode`, a TS enum generated from the table
`bevy_ts::keys::KEY_CODES`. Run conditions are JS builders (`{ condition: true, kind, ... }`)
lowered to `ConditionDecl` in `crates/bevy-ts/src/manifest.rs`, compiled and evaluated in Rust in
`crates/bevy-ts/src/systems.rs`. The engine already compiles `mouse`, `touch`, `gestures`,
`gamepad` and `bevy_gilrs`. Research: `docs/research/script-input-devices.md`.

Neighbours, agreed during research: rust-events owns reading and writing every message by type;
rust-resources owns `Res<T>` of Rust resources; ecs-schedules owns condition combinators;
writable-native-params owns the `#[export]` rules.

## Goals / Non-Goals

**Goals:**
- Mouse buttons, cursor, touch and gamepad state from TS, with Bevy's names and methods.
- Bevy's four input conditions, evaluated in Rust.
- A coverage table that accounts for every Bevy 0.20 input type.
- Headless tests for all of it.

**Non-Goals:**
- A message reader or writer (rust-events), `Res<T>` (rust-resources), combinators (ecs-schedules).
- Writing `GamepadSettings`; rumble in viewport mode; logical keys (`ButtonInput<Key>`). Follow-ups.
- A resource or system of our own: every param reads Bevy's state.

## Coverage

Every public Bevy 0.20 input type, its TS path and its owner. Stage 1.11 copies this table into
`docs/native.md`.

| Bevy type | TS path | Owner |
|---|---|---|
| `ButtonInput<KeyCode>`, `KeyCode` | `Keys`, `KeyCode` enum | text-input |
| `InputFocus` | `InputFocus` | text-input |
| `ButtonInput<Key>`, `Key` (logical keys) | none yet | follow-up (unowned) |
| `KeyboardInput`, `KeyboardFocusLost` | `MessageReader` | rust-events |
| `ButtonInput<MouseButton>`, `MouseButton` | `MouseButtons`, `MouseButton` enum | this change |
| `AccumulatedMouseMotion`, `AccumulatedMouseScroll`, `MouseScrollPixelsPerLine`, `MouseScrollUnit` | `Res<T>` | rust-resources (path); this change, stage 2 (docs, test) |
| `MouseButtonInput`, `MouseMotion`, `MouseWheel` | `MessageReader` | rust-events; this change, stage 3 (docs, test) |
| `Window::cursor_position`, `physical_cursor_position` | `Cursor` | this change |
| `CursorMoved`, `CursorEntered`, `CursorLeft` | `MessageReader` | rust-events; stage 3 |
| `Touches`, `Touch` | `Touches`, `Touch` record (no `force`) | this change |
| `TouchInput`, `TouchPhase`, `ForceTouch` | `MessageReader` | rust-events; stage 3 |
| `PinchGesture`, `RotationGesture`, `DoubleTapGesture`, `PanGesture` | `MessageReader` | rust-events; stage 3 |
| `Gamepad` | `With<Gamepad>` to find pads; `Gamepads` for its methods | this change |
| `GamepadButton`, `GamepadAxis`, `GamepadInput` | enums; `GamepadInput` as either | this change |
| `GamepadSettings`, `ButtonSettings`, `AxisSettings`, `ButtonAxisSettings` | read: reflection; write: none | follow-up (reflect-coverage / writable-native-params) |
| `GamepadEvent`, `GamepadConnectionEvent`, `GamepadButtonChangedEvent`, `GamepadButtonStateChangedEvent`, `GamepadAxisChangedEvent`, `RawGamepadEvent` and its parts | `MessageReader` | rust-events; stage 3 |
| `GamepadRumbleRequest`, `GamepadRumbleIntensity` | `MessageWriter` | rust-events (path); stage 3 (docs, test) |
| Rumble in viewport mode | none: the viewport protocol has no rumble | follow-up |
| `input_pressed`, `input_just_pressed`, `input_just_released`, `input_toggle_active` | `inputPressed` etc. over `KeyCode` / `MouseButton` | this change |
| Condition combinators | `not`, `and`, `or` ... | ecs-schedules |
| `ButtonState`, `Axis<T>` | through the params | — |
| `Gilrs`, `GilrsGamepads`, `RunningRumbleEffects`, `RumbleSystems` | none: backend internals | — |

Bevy has no gamepad condition: gamepad buttons are per entity, not a `ButtonInput` resource.

## Decisions

### Params mirror Bevy's API where it is methods over hidden data

Exported params only where TS cannot see the data: `ButtonInput` (a `HashSet`), `Touches` (no
`Reflect`), `Gamepad` (private fields), `Window::cursor_position` (private state). Plain-data
resources (`AccumulatedMouseMotion` and friends) go through `Res<T>`: one path, Bevy's own field
names. Each param follows `KeysParam`: `Option<Res<..>>` or a read-only `Query`, numbers in,
`Result<_, String>` errors, no resource reads as nothing.

`Cursor` is `Query<&Window, With<PrimaryWindow>>`. Its name avoids `Window` and `PrimaryWindow`,
which `native.d.ts` already declares as reflected classes; Bevy has no `Cursor` type since 0.15.

`Gamepads` is `Query<&Gamepad>`, entity first, so pads are found as in Bevy with `With<Gamepad>`.
Alternative: one param per pad. Rejected: TS has no per-entity param.

### One number space for input enums

`KeyCode` keeps text-input's `0..`; `MouseButton` starts at `0x1000`, `GamepadButton` at `0x2000`,
`GamepadAxis` at `0x2100`. The tables sit beside `KEY_CODES` in `bevy_ts::keys`, the one owner of
input numbers, because `systems.rs` (crate `bevy-ts`) decodes `KeyCode` and `MouseButton` for
conditions. So `inputJustPressed(KeyCode.Space)` and `inputJustPressed(MouseButton.Left)` share one
function, and `gamepads.get(pad, GamepadButton.LeftTrigger2)` / `get(pad, GamepadAxis.LeftStickX)`
keep Bevy's one `get`. Alternative: a device tag in each condition (`inputJustPressed.mouse(..)`).
Rejected: Bevy's names would not stand. Decided by the lead.

### Touch lists by index

`#[export]` returns no lists. `Touches` adds `count()`/`at(i)` and `justPressed`/`justReleased`/
`justCanceled` `Count()`/`At(i)`, over Bevy's iterators. Bevy's `HashMap` order is stable while the
resource is borrowed, so indices are stable within one run. Alternative: `Vec<HostRecord>` returns,
a new export pattern. Rejected with lead@writable-native-params, who is not adding list returns.

`Touch` is a `HostRecord` built from Bevy's getters. `force` is left out (an enum with data); the
`TouchInput` message carries it. Ids are `u64` passed as numbers; winit ids are small.

### Conditions are four `ConditionDecl` arms

`InputPressed { code }`, `InputJustPressed { code }`, `InputJustReleased { code }`,
`InputToggleActive { default, code }`. `compile_condition` decodes the number into a `KeyCode` or
`MouseButton` (a gamepad or unknown number fails the load) and `evaluate` reads the matching
`ButtonInput`. Each arm stands alone, so ecs-schedules' combinators wrap them unchanged; whoever
lands second rebases. The JS builders live in `bevy-ts/input`'s generated JS, beside the enums.

`inputToggleActive` keeps its bit in the system's `SystemMemory` entry, as `onMessage` keeps its
cursor there. `evaluate` takes `&World`, so the toggle flips in a step before evaluation that has
the memory mutably; the flip happens once per frame per system, when its condition is checked, as
Bevy's `Local<bool>` flips when its condition runs.

### Tests are headless Rust

`crates/bevy-ts-cli/tests/input_devices.rs`, not macOS-gated: `MinimalPlugins`, `InputPlugin`,
`WindowPlugin` with a spawned primary window, `BevyTsPlugin`, `ScriptInputPlugin`, no
`GilrsPlugin`. Each test writes the messages Bevy's input systems consume
(`MouseButtonInput`, `TouchInput`, `RawGamepadEvent`), or calls `Window::set_cursor_position`,
runs `app.update()`, and asserts on a TS resource. Conditions count runs and check no JS call, as
`a_skipped_system_costs_no_js_call`.

## Risks / Trade-offs

- Two changes edit `ConditionDecl` (this and ecs-schedules) → arms are self-contained; the second to
  land rebases.
- Stages 2 and 3 depend on other teams → their tasks are docs and tests only; stage 1 stands alone.
- Bevy adds an input variant → the enum tests fail, naming it.
- `Touch` ids above 2^53 lose precision → winit ids are small; documented.

## Migration Plan

Additive. Stage 1 lands after text-input; stages 2 and 3 land as their dependencies do.

## Open Questions

- How rust-events spells `Duration` in `GamepadRumbleRequest` (reflect-coverage's call). Stage 3
  documents whatever it chose.
