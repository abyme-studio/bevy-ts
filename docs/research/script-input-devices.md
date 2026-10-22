# script-input-devices: research

Research for coverage audit row 7: mouse, touch, gestures, gamepads and input conditions from TS.
Read 2026-10-23 from the cargo registry (bevy_input, bevy_gilrs, bevy_window 0.20.0), text-input's
branch, and the leads of rust-events, rust-resources, ecs-schedules and text-input.

## 1. Bevy 0.20 inventory

`InputPlugin` adds all of it per cargo feature; `bevy-ts-cli` already has `mouse`, `touch`,
`gestures`, `gamepad` and `bevy_gilrs` on, through bevy's `3d` feature (`cargo tree -e
features,no-dev -i bevy_gilrs`). Systems run in `PreUpdate` in `InputSystems`.

| Item | Kind | Shape | TS path | Owner |
|---|---|---|---|---|
| `ButtonInput<MouseButton>` | resource | methods over a `HashSet` | exported param `MouseButtons` | this change |
| `MouseButton` | enum | `Left Right Middle Back Forward Other(u16)` | generated TS enum, `Other` left out (as `KeyCode::Unidentified`) | this change |
| `AccumulatedMouseMotion` | resource | `{ delta: Vec2 }` | `Res<AccumulatedMouseMotion>`, layout tier | rust-resources (path); this change (docs, test) |
| `AccumulatedMouseScroll` | resource | `{ unit: MouseScrollUnit, delta: Vec2 }` | as above | as above |
| `MouseScrollPixelsPerLine` | resource | `f32` newtype | as above | as above |
| `MouseButtonInput`, `MouseMotion`, `MouseWheel` | messages | plain data | `MessageReader` | rust-events |
| `Window::cursor_position()` / `physical_cursor_position()` | method on the `Window` component | private `InternalWindowState`, not a column | exported param `Cursor` | this change |
| `CursorMoved`, `CursorEntered`, `CursorLeft` | messages (bevy_window) | plain data | `MessageReader` | rust-events |
| `Touches` | resource, **no `Reflect`** | `HashMap`s of `Touch` | exported param `Touches` | this change |
| `Touch` | struct, private fields, getters | `id position start_position previous_position delta() distance() force()` | `HostRecord` | this change |
| `TouchInput`, `TouchPhase`, `ForceTouch` | message, enums | plain data | `MessageReader` | rust-events |
| `PinchGesture(f32)`, `RotationGesture(f32)`, `DoubleTapGesture`, `PanGesture(Vec2)` | messages only, no state | plain data | `MessageReader` | rust-events (this change: docs, test) |
| `Gamepad` | component on the pad's entity | private `ButtonInput<GamepadButton>`, `Axis<GamepadInput>`, ids; reflect tier | `With<Gamepad>` to find pads; exported param `Gamepads` for its methods | this change |
| `GamepadButton` (19 + `Other`), `GamepadAxis` (6 + `Other`), `GamepadInput` | enums | | generated TS enums | this change |
| `GamepadSettings`, `ButtonSettings`, `AxisSettings`, `ButtonAxisSettings` | required component | dead zones, thresholds | read: reflect; write: needs a writing path | later (reflect-coverage / writable-native-params) |
| `GamepadEvent`, `GamepadConnectionEvent`, `GamepadButtonChangedEvent`, `GamepadButtonStateChangedEvent`, `GamepadAxisChangedEvent`, `RawGamepad*` | messages | | `MessageReader` | rust-events |
| `GamepadRumbleRequest { Add { duration, intensity, gamepad } , Stop { gamepad } }`, `GamepadRumbleIntensity` | message (write) | enum with `Duration`, nested struct | `MessageWriter` | rust-events (path); this change (docs, test) |
| `Gilrs`, `GilrsGamepads`, `RunningRumbleEffects`, `RumbleSystems` | bevy_gilrs internals | | none (backend) | — |
| `input_pressed`, `input_just_pressed`, `input_just_released`, `input_toggle_active` | conditions over `Res<ButtonInput<T>>` | `T` = `KeyCode`, `Key`, `MouseButton` | Rust-evaluated `ConditionDecl` variants | this change |
| `ButtonState`, `Axis<T>` | shared types | | via the params | — |

No gamepad condition exists in Bevy: gamepad buttons are per entity, not a `ButtonInput` resource.

## 2. text-input's `bevy-ts/input`

`crates/bevy-ts-cli/src/input.rs` on branch `text-input`: `ScriptInputPlugin` registers
`ts_param::<KeysParam>()`, `ts_param::<FocusParam>()` and `ts_module("bevy-ts/input", js, dts)`.

- `Keys`: `#[derive(SystemParam)] struct KeysParam { keys: Option<Res<ButtonInput<KeyCode>>> }`,
  `#[export(ts = "Keys", module = "bevy-ts/input")]`, methods `pressed`, `justPressed`,
  `justReleased`, each `(code: u32) -> Result<bool, String>`. No resource reads as nothing down.
- `KeyCode`: a frozen JS object and a `declare enum`, numbered from the one table `bevy_ts::keys`;
  a bad number throws naming the param, method and number. A test checks list = TS = Bevy.
- No resource or system of its own.

Mouse and gamepads follow this exactly: `Option<Res<..>>` / a read-only `Query`, numbers in,
`Result` errors, generated enums. lead@text-input: final gate running, lands on main within hours,
and asks that nobody edits `input.rs` before then.

## 3. The line with rust-events (agreed with lead@rust-events)

- rust-events owns reading and writing every input message, generically by reflect type path.
  This change adds no reader; it tests and documents the input ones.
- This change owns the state params in `bevy-ts/input` and the input conditions.
- rust-resources (agreed with research@rust-resources) owns `Res<T>` of Rust resources. Plain-data
  input resources (`AccumulatedMouseMotion`, `AccumulatedMouseScroll`, `MouseScrollPixelsPerLine`)
  go through it, with no exported param of ours: one path, Bevy's own `motion.delta`. Exported
  params only where Bevy's API is methods over data TS cannot see: `ButtonInput`, `Touches`,
  `Gamepad`, `Window::cursor_position`.

## 4. Rumble

`MessageWriter<GamepadRumbleRequest>` from rust-events, as Bevy's `gamepad_rumble` example
writes it. Not writable-native-params. Their writer takes a message as JSON through
reflect-coverage's `reflect_value`, so the enum, `GamepadRumbleIntensity` and `Duration` need no
helper; how `Duration` is spelt is reflect-coverage's call (open). Build of the writer waits on an
operator OK for per-message serialization cost and on reflect-coverage stage 1. Viewport mode has
no rumble in its protocol: a request reaches gilrs only. That gap is out of scope here.

## 5. Conditions (agreed with lead@ecs-schedules)

ecs-schedules owns the machinery and the combinators (`or`, `and`, `runOnce`, `resource*`, a TS
predicate). This change adds only `inputPressed`, `inputJustPressed`, `inputJustReleased` and
`inputToggleActive`, each one self-contained `ConditionDecl` arm in `bevy-ts/src/manifest.rs`,
compiled and evaluated in `systems.rs` against `ButtonInput<T>`, so a skipped system costs no JS
call (as `a_skipped_system_costs_no_js_call`). `inputToggleActive` keeps its bit in the system's
`SystemMemory`. Whoever lands second rebases. A TS predicate is not enough: it pays a JS call each
frame, which is the cost Rust conditions exist to avoid.

`inputJustPressed(KeyCode.Space)` and `inputJustPressed(MouseButton.Left)` must tell the device
from the number. Recommendation: the generated enums take disjoint ranges from one table
(`KeyCode` 0.. as text-input ships it, `MouseButton` from 0x1000, `GamepadButton` 0x2000,
`GamepadAxis` 0x2100). It also lets `gamepads.get(pad, GamepadButton.LeftTrigger2 |
GamepadAxis.LeftStickX)` keep Bevy's one `get(impl Into<GamepadInput>)`.

## The proposed TS shape

```ts
import { MouseButton, GamepadButton, GamepadAxis, inputJustPressed,
  type MouseButtons, type Cursor, type Touches, type Gamepads } from "bevy-ts/input";

export function shoot(mouse: MouseButtons, cursor: Cursor) {
  if (mouse.justPressed(MouseButton.Left)) aim(cursor.position()); // Vec2 | undefined
}
export function look(motion: Res<AccumulatedMouseMotion>) { yaw -= motion.delta.x; }
export function pads(q: Query<[Entity], With<Gamepad>>, gamepads: Gamepads) {
  for (const [pad] of q) {
    if (gamepads.justPressed(pad, GamepadButton.South)) jump();
    const s = gamepads.leftStick(pad);        // Vec2
  }
}
export function pinch(touches: Touches) {
  if (touches.anyJustPressed()) start(touches.firstPressedPosition());
}
app.addSystems(Update, systems(pause).runIf(inputJustPressed(KeyCode.Escape)));
```

- `MouseButtons`: `pressed`, `justPressed`, `justReleased`, as `Keys`.
- `Cursor` over `Query<&Window, With<PrimaryWindow>>`: `position()` and `physicalPosition()` of the
  primary window. The name is a spec call: `Window` and `PrimaryWindow` are already reflected
  classes in `native.d.ts`.
- `Gamepads` over `Query<&Gamepad>`: `pressed`, `justPressed`, `justReleased`, `get`,
  `getUnclamped`, `leftStick`, `rightStick`, `dpad`, `vendorId`, `productId`, each taking the pad
  entity first; an entity with no `Gamepad` throws.
- `Touches` over `Res<Touches>`: `anyJustPressed`, `anyJustReleased`, `anyJustCanceled`,
  `justPressed(id)`, `justReleased(id)`, `justCanceled(id)`, `getPressed(id)`, `getReleased(id)`,
  `firstPressedPosition()`, and iteration (below). `Touch` is a `HostRecord`; ids are `u64`, so
  numbers lose precision past 2^53 (winit ids are small).

Open: iteration. `#[export]` returns no lists, and Bevy's touch example uses
`touches.iter_just_pressed()`. Options: let `#[export]` return `Vec<HostRecord>` as a JS array
(cleaner; changes the macro's type rule, a small extension of an existing pattern), or index
accessors (`len()`, `at(i)`). Recommend the `Vec` return, settled with whoever owns
`bevy-ts-native`'s export rules (writable-native-params is also changing them).

## 6. Headless tests

Yes, all of it. Bevy's own tests do this. A Rust test in `crates/bevy-ts-cli/tests/`, not
macOS-gated: `MinimalPlugins` + `InputPlugin` (+ `WindowPlugin` with a spawned `Window` for the
cursor), `BevyTsPlugin`, `ScriptInputPlugin`, no `GilrsPlugin`. Write the messages Bevy's input
systems consume with `world.write_message`, `app.update()`, assert on a TS resource:

- mouse: `MouseButtonInput`; motion and scroll: `MouseMotion` / `MouseWheel` (accumulated by
  Bevy's own systems).
- cursor: `Window::set_cursor_position` on the spawned window.
- touch: `TouchInput` with `TouchPhase::Started`/`Moved`/`Ended`; gestures: write `PinchGesture`.
- gamepad: spawn an entity, write `RawGamepadEvent::Connection(GamepadConnectionEvent::new(e,
  Connected { .. }))`, then `RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(e, South,
  1.0))` and an axis event; Bevy's processing systems fill `Gamepad`.
- rumble: the TS system writes a request; the test drains `Messages<GamepadRumbleRequest>`.
- conditions: count runs of a system under `inputJustPressed` and check no JS call when false.

End to end, the macOS viewport path already turns viewer pointer, wheel, touch and gamepad events
into these same messages (`crates/bevy-ts-viewport/src/input.rs`), so one viewport test like
text-input's covers the real route.

## 7. Ask-first

- New dependency: none. gilrs and every input feature are already compiled into the engine.
- Performance: none expected. Each method is one fast op, and the conditions remove JS calls.
- New pattern, worth the lead's eye: `Vec<HostRecord>` returns in `#[export]` (if chosen), and
  disjoint number ranges for the generated input enums. Both extend existing patterns.
- Inherited, not ours: rust-events' writer waits on an operator OK for its serialization cost.

## Staging

1. Now, after text-input lands: `MouseButtons`, `Cursor`, `Gamepads`, `Touches`, the generated
   `MouseButton`/`GamepadButton`/`GamepadAxis` enums, the four input conditions, headless tests.
   Needs nothing from rust-events, rust-resources, writable-native-params or ecs-schedules.
2. After rust-resources lands `Res<T>` of Bevy resources: docs and a test for
   `Res<AccumulatedMouseMotion>` / `Res<AccumulatedMouseScroll>`.
3. After rust-events lands its reader and writer (with reflect-coverage stage 1): docs and tests
   for the input messages, gestures, and rumble through `MessageWriter<GamepadRumbleRequest>`.
4. Later, not this change: writing `GamepadSettings`; rumble in the viewport protocol.
