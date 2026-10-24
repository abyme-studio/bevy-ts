//! `bevy-ts/input`, a built-in module every engine ships: exported params over Bevy's own input
//! state (`Keys`, `LogicalKeys`, `InputFocus`, `InputFocusVisible`, `DirectionalNavigation`,
//! `MouseButtons`, `Cursor`, `Gamepads`, `Touches`), the enums `KeyCode`, `MouseButton`,
//! `GamepadButton` and `GamepadAxis`, generated from `bevy_ts::keys`, and `CompassOctant`, and
//! `Key`, Bevy's logical key as one number.
//! A TS system reads input as a Rust one does:
//! `keys.justPressed(KeyCode.KeyN) && focus.get() === undefined`.

use bevy::ecs::schedule::BoxedCondition;
use bevy::ecs::system::SystemParam;
use bevy::input::gamepad::{Gamepad, GamepadInput};
use bevy::input::keyboard::Key;
use bevy::input::touch::{Touch, Touches};
use bevy::input_focus::{InputFocus, InputFocusVisible};
use bevy::math::CompassOctant;
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigator;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_ts::keys::{
    CHARACTER_BASE, DEAD_KEY_BASE, DEAD_KEY_NONE, GAMEPAD_AXES, GAMEPAD_AXIS_BASE, GAMEPAD_BUTTON_BASE, GAMEPAD_BUTTONS,
    InputCode, KEY_BASE, KEY_CODES, MOUSE_BUTTON_BASE, MOUSE_BUTTONS, NAMED_KEYS, input_code_of, key_code_of, key_of, names,
};
use bevy_ts_native::prelude::*;
use std::fmt::Write as _;

pub const MODULE: &str = "bevy-ts/input";

/// Registers the module and its params.
pub struct ScriptInputPlugin;

impl Plugin for ScriptInputPlugin {
    fn build(&self, app: &mut App) {
        app.ts_param::<KeysParam<'static>>()
            .ts_param::<LogicalKeysParam<'static>>()
            .ts_param::<FocusParam<'static>>()
            .ts_param::<FocusVisibleParam<'static>>()
            .ts_param::<NavigationParam<'static, 'static>>()
            .ts_param::<MouseButtonsParam<'static>>()
            .ts_param::<CursorParam<'static, 'static>>()
            .ts_param::<GamepadsParam<'static, 'static>>()
            .ts_param::<TouchesParam<'static>>()
            .ts_condition("input:pressed", |a| condition(a, "inputPressed", Cond::Pressed))
            .ts_condition("input:just-pressed", |a| condition(a, "inputJustPressed", Cond::JustPressed))
            .ts_condition("input:just-released", |a| condition(a, "inputJustReleased", Cond::JustReleased))
            .ts_condition("input:toggle-active", |a| {
                let default = a["default"].as_bool().ok_or("inputToggleActive takes (default: boolean, code)")?;
                condition(a, "inputToggleActive", Cond::ToggleActive(default))
            })
            .ts_module(MODULE, &js(), &dts());
    }
}

/// `Keys`: `Res<ButtonInput<KeyCode>>`. No resource (no input plugin) reads as no key down.
#[derive(SystemParam)]
pub struct KeysParam<'w> {
    keys: Option<Res<'w, ButtonInput<KeyCode>>>,
}

fn code(n: u32) -> Result<KeyCode, String> {
    key_code_of(n).ok_or_else(|| format!("{n} is not a KeyCode; use KeyCode.KeyA and friends from bevy-ts/input"))
}

#[export(ts = "Keys", module = "bevy-ts/input")]
impl KeysParam<'_> {
    /// Whether the key is down.
    pub fn pressed(&self, code: u32) -> Result<bool, String> {
        let k = self::code(code)?;
        Ok(self.keys.as_ref().is_some_and(|keys| keys.pressed(k)))
    }

    /// Whether the key went down this frame.
    pub fn just_pressed(&self, code: u32) -> Result<bool, String> {
        let k = self::code(code)?;
        Ok(self.keys.as_ref().is_some_and(|keys| keys.just_pressed(k)))
    }

    /// Whether the key went up this frame.
    pub fn just_released(&self, code: u32) -> Result<bool, String> {
        let k = self::code(code)?;
        Ok(self.keys.as_ref().is_some_and(|keys| keys.just_released(k)))
    }
}

/// `LogicalKeys`: `Res<ButtonInput<Key>>`, the key by what it types. No resource reads as no key
/// down.
#[derive(SystemParam)]
pub struct LogicalKeysParam<'w> {
    keys: Option<Res<'w, ButtonInput<Key>>>,
}

fn key(n: u32) -> Result<Key, String> {
    key_of(n).ok_or_else(|| not_a(n, "Key", "Key.Enter, Key.Character(\"z\")"))
}

#[export(ts = "LogicalKeys", module = "bevy-ts/input")]
impl LogicalKeysParam<'_> {
    /// Whether the key is down.
    pub fn pressed(&self, key: u32) -> Result<bool, String> {
        let k = self::key(key)?;
        Ok(self.keys.as_ref().is_some_and(|keys| keys.pressed(k)))
    }

    /// Whether the key went down this frame.
    pub fn just_pressed(&self, key: u32) -> Result<bool, String> {
        let k = self::key(key)?;
        Ok(self.keys.as_ref().is_some_and(|keys| keys.just_pressed(k)))
    }

    /// Whether the key went up this frame.
    pub fn just_released(&self, key: u32) -> Result<bool, String> {
        let k = self::key(key)?;
        Ok(self.keys.as_ref().is_some_and(|keys| keys.just_released(k)))
    }
}

/// `InputFocus`: `Res<InputFocus>`.
#[derive(SystemParam)]
pub struct FocusParam<'w> {
    focus: Option<Res<'w, InputFocus>>,
}

#[export(ts = "InputFocus", module = "bevy-ts/input")]
impl FocusParam<'_> {
    /// The focused entity, or `undefined` while nothing has focus.
    pub fn get(&self) -> Option<Entity> {
        self.focus.as_ref().and_then(|f| f.get())
    }
}

/// `InputFocusVisible`: `Res<InputFocusVisible>`, whether to draw a focus ring (Bevy sets it when
/// Tab moves focus, and clears it on a press).
#[derive(SystemParam)]
pub struct FocusVisibleParam<'w> {
    visible: Option<Res<'w, InputFocusVisible>>,
}

#[export(ts = "InputFocusVisible", module = "bevy-ts/input")]
impl FocusVisibleParam<'_> {
    /// Whether the focused entity should show that it has focus.
    pub fn get(&self) -> bool {
        self.visible.as_ref().is_some_and(|v| v.0)
    }
}

/// `DirectionalNavigation`: bevy_ui's `AutoDirectionalNavigator`, manual edges of the
/// `DirectionalNavigationMap` first, then the nearest `AutoDirectionalNavigation` node. `None`
/// without the map, which only `"directionalNavigation": true` adds.
#[derive(SystemParam)]
pub struct NavigationParam<'w, 's> {
    nav: Option<AutoDirectionalNavigator<'w, 's>>,
}

/// Bevy's eight `CompassOctant`s, in its order: the TS enum's members.
const OCTANTS: [(&str, CompassOctant); 8] = [
    ("North", CompassOctant::North),
    ("NorthEast", CompassOctant::NorthEast),
    ("East", CompassOctant::East),
    ("SouthEast", CompassOctant::SouthEast),
    ("South", CompassOctant::South),
    ("SouthWest", CompassOctant::SouthWest),
    ("West", CompassOctant::West),
    ("NorthWest", CompassOctant::NorthWest),
];

/// `CompassOctant`'s numbers follow the input enums', so no number means two things.
const OCTANT_BASE: u32 = GAMEPAD_AXIS_BASE + GAMEPAD_AXES.len() as u32;

fn octant(n: u32) -> Result<CompassOctant, String> {
    n.checked_sub(OCTANT_BASE).and_then(|i| OCTANTS.get(i as usize)).map(|(_, o)| *o).ok_or_else(|| not_a(n, "CompassOctant", "CompassOctant.South"))
}

#[export(ts = "DirectionalNavigation", module = "bevy-ts/input")]
impl NavigationParam<'_, '_> {
    /// Moves `InputFocus` to the neighbour in `direction` and returns it; `undefined` when nothing
    /// has focus or there is no neighbour that way.
    pub fn navigate(&mut self, direction: u32) -> Result<Option<Entity>, String> {
        let d = octant(direction)?;
        let nav = self.nav.as_mut().ok_or("DirectionalNavigation has no map: set \"directionalNavigation\": true in bevy-ts.json")?;
        Ok(nav.navigate(d).ok())
    }
}

/// Bevy's four input conditions, as `bevy-ts/input` exposes them.
#[derive(Clone, Copy)]
enum Cond {
    Pressed,
    JustPressed,
    JustReleased,
    ToggleActive(bool),
}

/// Builds Bevy's condition `c` over `ButtonInput<KeyCode>` or `ButtonInput<MouseButton>`, chosen
/// by the range of the call's `code`. Gamepad numbers fail: a pad's buttons are its entity's.
fn condition(args: &serde_json::Value, call: &str, c: Cond) -> Result<BoxedCondition, String> {
    fn of<T: Clone + Eq + std::hash::Hash + Send + Sync + 'static>(input: T, c: Cond) -> BoxedCondition {
        use bevy::input::common_conditions::*;
        match c {
            Cond::Pressed => Box::new(IntoSystem::into_system(input_pressed(input))),
            Cond::JustPressed => Box::new(IntoSystem::into_system(input_just_pressed(input))),
            Cond::JustReleased => Box::new(IntoSystem::into_system(input_just_released(input))),
            Cond::ToggleActive(default) => Box::new(IntoSystem::into_system(input_toggle_active(default, input))),
        }
    }
    let n = args["code"].as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| format!("{call} takes a KeyCode or a MouseButton"))?;
    match input_code_of(n) {
        Some(InputCode::Key(k)) => Ok(of(k, c)),
        Some(InputCode::Mouse(b)) => Ok(of(b, c)),
        Some(InputCode::GamepadButton(_) | InputCode::GamepadAxis(_)) => Err(format!(
            "{call}({n}): {n} is a gamepad input, and Bevy has no gamepad condition; read a pad's buttons with Gamepads in the system"
        )),
        None => Err(not_a(n, "KeyCode or MouseButton", "KeyCode.Space")),
    }
}

/// The error for a number that is not of the enum `want`, naming the enum it is of, if any.
fn not_a(n: u32, want: &str, example: &str) -> String {
    let of = match input_code_of(n) {
        Some(InputCode::Key(_)) => " (a KeyCode)",
        Some(InputCode::Mouse(_)) => " (a MouseButton)",
        Some(InputCode::GamepadButton(_)) => " (a GamepadButton)",
        Some(InputCode::GamepadAxis(_)) => " (a GamepadAxis)",
        None if key_of(n).is_some() => " (a Key)",
        None => "",
    };
    format!("{n}{of} is not a {want}; use {example} and friends from bevy-ts/input")
}

/// `MouseButtons`: `Res<ButtonInput<MouseButton>>`. No resource reads as no button down.
#[derive(SystemParam)]
pub struct MouseButtonsParam<'w> {
    buttons: Option<Res<'w, ButtonInput<MouseButton>>>,
}

fn mouse_button(n: u32) -> Result<MouseButton, String> {
    match input_code_of(n) {
        Some(InputCode::Mouse(b)) => Ok(b),
        _ => Err(not_a(n, "MouseButton", "MouseButton.Left")),
    }
}

#[export(ts = "MouseButtons", module = "bevy-ts/input")]
impl MouseButtonsParam<'_> {
    /// Whether the button is down.
    pub fn pressed(&self, button: u32) -> Result<bool, String> {
        let b = mouse_button(button)?;
        Ok(self.buttons.as_ref().is_some_and(|m| m.pressed(b)))
    }

    /// Whether the button went down this frame.
    pub fn just_pressed(&self, button: u32) -> Result<bool, String> {
        let b = mouse_button(button)?;
        Ok(self.buttons.as_ref().is_some_and(|m| m.just_pressed(b)))
    }

    /// Whether the button went up this frame.
    pub fn just_released(&self, button: u32) -> Result<bool, String> {
        let b = mouse_button(button)?;
        Ok(self.buttons.as_ref().is_some_and(|m| m.just_released(b)))
    }
}

/// `Cursor`: the cursor over the window with `PrimaryWindow`. Not `Window`, which TS already has
/// as a reflected component.
#[derive(SystemParam)]
pub struct CursorParam<'w, 's> {
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
}

#[export(ts = "Cursor", module = "bevy-ts/input")]
impl CursorParam<'_, '_> {
    /// Bevy's `Window::cursor_position`: logical pixels from the window's top-left, or
    /// `undefined` when the cursor is outside it or there is no primary window.
    pub fn position(&self) -> Option<Vec2> {
        self.windows.single().ok().and_then(Window::cursor_position)
    }

    /// Bevy's `Window::physical_cursor_position`, in physical pixels.
    pub fn physical_position(&self) -> Option<Vec2> {
        self.windows.single().ok().and_then(Window::physical_cursor_position)
    }
}

/// `Gamepads`: every `Gamepad` component, Bevy's per-pad state. Each method takes the pad's
/// entity first; find pads with `Query<[Entity], With<Gamepad>>`.
#[derive(SystemParam)]
pub struct GamepadsParam<'w, 's> {
    pads: Query<'w, 's, &'static Gamepad>,
}

fn gamepad_button(n: u32) -> Result<GamepadButton, String> {
    match input_code_of(n) {
        Some(InputCode::GamepadButton(b)) => Ok(b),
        _ => Err(not_a(n, "GamepadButton", "GamepadButton.South")),
    }
}

fn gamepad_input(n: u32) -> Result<GamepadInput, String> {
    match input_code_of(n) {
        Some(InputCode::GamepadButton(b)) => Ok(GamepadInput::Button(b)),
        Some(InputCode::GamepadAxis(a)) => Ok(GamepadInput::Axis(a)),
        _ => Err(not_a(n, "GamepadButton or GamepadAxis", "GamepadAxis.LeftStickX")),
    }
}

impl GamepadsParam<'_, '_> {
    fn pad(&self, pad: Entity) -> Result<&Gamepad, String> {
        self.pads.get(pad).map_err(|_| format!("entity {} is not a gamepad", pad.to_bits()))
    }
}

#[export(ts = "Gamepads", module = "bevy-ts/input")]
impl GamepadsParam<'_, '_> {
    /// Whether the pad's button is down.
    pub fn pressed(&self, pad: Entity, button: u32) -> Result<bool, String> {
        let b = gamepad_button(button)?;
        Ok(self.pad(pad)?.pressed(b))
    }

    /// Whether the pad's button went down this frame.
    pub fn just_pressed(&self, pad: Entity, button: u32) -> Result<bool, String> {
        let b = gamepad_button(button)?;
        Ok(self.pad(pad)?.just_pressed(b))
    }

    /// Whether the pad's button went up this frame.
    pub fn just_released(&self, pad: Entity, button: u32) -> Result<bool, String> {
        let b = gamepad_button(button)?;
        Ok(self.pad(pad)?.just_released(b))
    }

    /// A button's or axis's value after the pad's settings: Bevy starts every standard input at
    /// `0` when the pad connects.
    pub fn get(&self, pad: Entity, input: u32) -> Result<Option<f32>, String> {
        let i = gamepad_input(input)?;
        Ok(self.pad(pad)?.get(i))
    }

    /// As `get`, without clamping to the pad's live zone.
    pub fn get_unclamped(&self, pad: Entity, input: u32) -> Result<Option<f32>, String> {
        let i = gamepad_input(input)?;
        Ok(self.pad(pad)?.get_unclamped(i))
    }

    /// The left stick, `LeftStickX` and `LeftStickY`.
    pub fn left_stick(&self, pad: Entity) -> Result<Vec2, String> {
        Ok(self.pad(pad)?.left_stick())
    }

    /// The right stick, `RightStickX` and `RightStickY`.
    pub fn right_stick(&self, pad: Entity) -> Result<Vec2, String> {
        Ok(self.pad(pad)?.right_stick())
    }

    /// The D-pad as a vector: right and up positive.
    pub fn dpad(&self, pad: Entity) -> Result<Vec2, String> {
        Ok(self.pad(pad)?.dpad())
    }

    /// The pad's USB vendor id, or `undefined` when its backend gives none.
    pub fn vendor_id(&self, pad: Entity) -> Result<Option<u16>, String> {
        Ok(self.pad(pad)?.vendor_id())
    }

    /// The pad's USB product id, or `undefined`.
    pub fn product_id(&self, pad: Entity) -> Result<Option<u16>, String> {
        Ok(self.pad(pad)?.product_id())
    }
}

/// A finger on the screen, Bevy's `Touch` without its force (read `TouchInput` for that).
/// Positions are logical pixels. `id` is winit's, passed as a number.
#[derive(HostRecord)]
pub struct TouchRecord {
    pub id: f64,
    pub position: Vec2,
    pub start_position: Vec2,
    pub previous_position: Vec2,
    pub delta: Vec2,
    pub distance: Vec2,
}

impl From<&Touch> for TouchRecord {
    fn from(t: &Touch) -> Self {
        TouchRecord {
            id: t.id() as f64,
            position: t.position(),
            start_position: t.start_position(),
            previous_position: t.previous_position(),
            delta: t.delta(),
            distance: t.distance(),
        }
    }
}

/// `Touches`: `Res<Touches>`. No resource reads as no touches. Bevy's iterators are `count()` and
/// `at(i)` pairs, in an order that holds for the whole run.
#[derive(SystemParam)]
pub struct TouchesParam<'w> {
    touches: Option<Res<'w, Touches>>,
}

/// A touch id from TS: a whole number that fits.
fn touch_id(id: f64) -> Result<u64, String> {
    if id >= 0.0 && id.fract() == 0.0 && id < u64::MAX as f64 { Ok(id as u64) } else { Err(format!("{id} is not a touch id")) }
}

impl TouchesParam<'_> {
    fn read<T: Default>(&self, f: impl FnOnce(&Touches) -> T) -> T {
        self.touches.as_deref().map(f).unwrap_or_default()
    }
}

#[export(ts = "Touches", module = "bevy-ts/input")]
impl TouchesParam<'_> {
    /// Whether any finger went down this frame.
    pub fn any_just_pressed(&self) -> bool {
        self.read(Touches::any_just_pressed)
    }

    /// Whether any finger lifted this frame.
    pub fn any_just_released(&self) -> bool {
        self.read(Touches::any_just_released)
    }

    /// Whether any touch was canceled this frame.
    pub fn any_just_canceled(&self) -> bool {
        self.read(Touches::any_just_canceled)
    }

    /// Whether finger `id` went down this frame.
    pub fn just_pressed(&self, id: f64) -> Result<bool, String> {
        let id = touch_id(id)?;
        Ok(self.read(|t| t.just_pressed(id)))
    }

    /// Whether finger `id` lifted this frame.
    pub fn just_released(&self, id: f64) -> Result<bool, String> {
        let id = touch_id(id)?;
        Ok(self.read(|t| t.just_released(id)))
    }

    /// Whether touch `id` was canceled this frame.
    pub fn just_canceled(&self, id: f64) -> Result<bool, String> {
        let id = touch_id(id)?;
        Ok(self.read(|t| t.just_canceled(id)))
    }

    /// Finger `id` while it is down, or `undefined`.
    pub fn get_pressed(&self, id: f64) -> Result<Option<TouchRecord>, String> {
        let id = touch_id(id)?;
        Ok(self.read(|t| t.get_pressed(id).map(TouchRecord::from)))
    }

    /// Finger `id` on the frame it lifted, or `undefined`.
    pub fn get_released(&self, id: f64) -> Result<Option<TouchRecord>, String> {
        let id = touch_id(id)?;
        Ok(self.read(|t| t.get_released(id).map(TouchRecord::from)))
    }

    /// The position of the first finger down, or `undefined` with none.
    pub fn first_pressed_position(&self) -> Option<Vec2> {
        self.read(Touches::first_pressed_position)
    }

    /// How many fingers are down: Bevy's `iter()`.
    pub fn count(&self) -> u32 {
        self.read(|t| t.iter().count() as u32)
    }

    /// The `i`th finger down, or `undefined` past `count()`.
    pub fn at(&self, i: u32) -> Option<TouchRecord> {
        self.read(|t| t.iter().nth(i as usize).map(TouchRecord::from))
    }

    /// How many fingers went down this frame: Bevy's `iter_just_pressed()`.
    pub fn just_pressed_count(&self) -> u32 {
        self.read(|t| t.iter_just_pressed().count() as u32)
    }

    /// The `i`th finger that went down this frame.
    pub fn just_pressed_at(&self, i: u32) -> Option<TouchRecord> {
        self.read(|t| t.iter_just_pressed().nth(i as usize).map(TouchRecord::from))
    }

    /// How many fingers lifted this frame: Bevy's `iter_just_released()`.
    pub fn just_released_count(&self) -> u32 {
        self.read(|t| t.iter_just_released().count() as u32)
    }

    /// The `i`th finger that lifted this frame.
    pub fn just_released_at(&self, i: u32) -> Option<TouchRecord> {
        self.read(|t| t.iter_just_released().nth(i as usize).map(TouchRecord::from))
    }

    /// How many touches were canceled this frame: Bevy's `iter_just_canceled()`.
    pub fn just_canceled_count(&self) -> u32 {
        self.read(|t| t.iter_just_canceled().count() as u32)
    }

    /// The `i`th touch canceled this frame.
    pub fn just_canceled_at(&self, i: u32) -> Option<TouchRecord> {
        self.read(|t| t.iter_just_canceled().nth(i as usize).map(TouchRecord::from))
    }
}

/// Each input enum: its TS name, doc, variant names and first number.
fn enums() -> [(&'static str, &'static str, Vec<&'static str>, u32); 5] {
    [
        ("KeyCode", "Bevy's `KeyCode`, the physical key, by its Rust name.", names(KEY_CODES), 0),
        ("MouseButton", "Bevy's `MouseButton`, by its Rust name; `Other(n)` has no member.", names(MOUSE_BUTTONS), MOUSE_BUTTON_BASE),
        ("GamepadButton", "Bevy's `GamepadButton`, by its Rust name; `Other(n)` has no member.", names(GAMEPAD_BUTTONS), GAMEPAD_BUTTON_BASE),
        ("GamepadAxis", "Bevy's `GamepadAxis`, by its Rust name; `Other(n)` has no member.", names(GAMEPAD_AXES), GAMEPAD_AXIS_BASE),
        ("CompassOctant", "Bevy's `CompassOctant`: a direction for `DirectionalNavigation.navigate`.", names(&OCTANTS), OCTANT_BASE),
    ]
}

/// The module's JS: each enum, one number per variant, numbers disjoint across enums.
fn js() -> String {
    let mut s = String::from("// `bevy-ts/input`: generated from bevy_ts::keys.\nimport { nativeCondition } from \"bevy-ts\";\n");
    for (name, _, variants, base) in enums() {
        let _ = writeln!(s, "export const {name} = Object.freeze({{");
        for (i, v) in variants.iter().enumerate() {
            let _ = writeln!(s, "  {v}: {},", base + i as u32);
        }
        s.push_str("});\n");
    }
    s.push_str(&key_js());
    for (i, (name, _)) in NAMED_KEYS.iter().enumerate() {
        let _ = writeln!(s, "  {name}: {},", KEY_BASE + i as u32);
    }
    s.push_str("});\n");
    s.push_str(CONDITIONS_JS);
    s
}

/// `Key`'s head: `Key.Character` and `Key.Dead` turn one character into its number here, so no
/// string reaches Rust. The named members follow.
fn key_js() -> String {
    format!(
        r#"function codePoint(fn, c) {{
  const cps = typeof c === "string" ? [...c.normalize("NFC")] : [];
  if (cps.length !== 1) throw new RangeError(`Key.${{fn}}(${{JSON.stringify(c)}}): takes one character, one code point after NFC`);
  return cps[0].codePointAt(0);
}}
export const Key = Object.freeze({{
  Character: (c) => {CHARACTER_BASE} + codePoint("Character", c),
  Dead: (c) => (c === undefined ? {DEAD_KEY_NONE} : {DEAD_KEY_BASE} + codePoint("Dead", c)),
"#
    )
}

const KEY_DTS: &str = r#"/** Bevy's `Key`, the logical key: what the key types under the layout and modifiers. */
export type Key = number & { readonly __brand: "Key" };
export declare namespace Key {
  /** `Key::Character(c)`; throws unless `c` is one code point after NFC. */
  function Character(c: string): Key;
  /** `Key::Dead(c)`, or `Key::Dead(None)` with no `c`. */
  function Dead(c?: string): Key;
"#;

/// Bevy's input conditions, evaluated in Rust: the conditions `ScriptInputPlugin` exposes.
const CONDITIONS_JS: &str = r#"export const inputPressed = (code) => nativeCondition("input:pressed", { code });
export const inputJustPressed = (code) => nativeCondition("input:just-pressed", { code });
export const inputJustReleased = (code) => nativeCondition("input:just-released", { code });
export const inputToggleActive = (initial, code) => nativeCondition("input:toggle-active", { default: initial, code });
"#;

const CONDITIONS_DTS: &str = r#"/** Bevy's `input_pressed`: true while the key or button is down. Evaluated in Rust. */
export declare function inputPressed(code: KeyCode | MouseButton): Condition;
/** Bevy's `input_just_pressed`: true on the frame the key or button goes down. */
export declare function inputJustPressed(code: KeyCode | MouseButton): Condition;
/** Bevy's `input_just_released`: true on the frame the key or button goes up. */
export declare function inputJustReleased(code: KeyCode | MouseButton): Condition;
/** Bevy's `input_toggle_active`: starts at `initial` and flips each frame the input is just pressed. */
export declare function inputToggleActive(initial: boolean, code: KeyCode | MouseButton): Condition;
"#;

/// The module's declarations; the params are declared from their `#[export]`.
fn dts() -> String {
    let mut s = String::from("import type { Condition } from \"bevy-ts\";\n");
    for (name, doc, variants, base) in enums() {
        let _ = writeln!(s, "/** {doc} */\nexport declare enum {name} {{");
        for (i, v) in variants.iter().enumerate() {
            let _ = writeln!(s, "  {v} = {},", base + i as u32);
        }
        s.push_str("}\n");
    }
    s.push_str(KEY_DTS);
    for (name, _) in NAMED_KEYS {
        let _ = writeln!(s, "  const {name}: Key;");
    }
    s.push_str("}\n");
    s.push_str(CONDITIONS_DTS);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compass_octants_follow_the_input_enums_and_decode() {
        for (i, (name, o)) in OCTANTS.iter().enumerate() {
            let n = OCTANT_BASE + i as u32;
            assert!(input_code_of(n).is_none(), "{n} is also an input code");
            assert!(js().contains(&format!("  {name}: {n},\n")) && dts().contains(&format!("  {name} = {n},\n")));
            assert_eq!(octant(n).unwrap(), *o);
        }
        assert!(octant(0).unwrap_err().contains("a KeyCode"));
    }

    /// Each TS enum is its Rust list: same names, same numbers, decoded back to the variant.
    #[test]
    fn the_ts_enums_are_the_lists() {
        for (i, (name, k)) in KEY_CODES.iter().enumerate() {
            assert!(js().contains(&format!("  {name}: {i},\n")));
            assert!(dts().contains(&format!("  {name} = {i},\n")));
            assert_eq!(code(i as u32), Ok(*k));
        }
        for (i, (name, b)) in MOUSE_BUTTONS.iter().enumerate() {
            let n = MOUSE_BUTTON_BASE + i as u32;
            assert!(js().contains(&format!("  {name}: {n},\n")) && dts().contains(&format!("  {name} = {n},\n")));
            assert_eq!(mouse_button(n), Ok(*b));
        }
        for (i, (name, b)) in GAMEPAD_BUTTONS.iter().enumerate() {
            let n = GAMEPAD_BUTTON_BASE + i as u32;
            assert!(js().contains(&format!("  {name}: {n},\n")) && dts().contains(&format!("  {name} = {n},\n")));
            assert_eq!(gamepad_button(n), Ok(*b));
            assert_eq!(gamepad_input(n), Ok(GamepadInput::Button(*b)));
        }
        for (i, (name, a)) in GAMEPAD_AXES.iter().enumerate() {
            let n = GAMEPAD_AXIS_BASE + i as u32;
            assert!(js().contains(&format!("  {name}: {n},\n")) && dts().contains(&format!("  {name} = {n},\n")));
            assert_eq!(gamepad_input(n), Ok(GamepadInput::Axis(*a)));
        }
        for e in ["KeyCode", "MouseButton", "GamepadButton", "GamepadAxis"] {
            assert!(js().contains(&format!("export const {e} = Object.freeze({{")));
            assert!(dts().contains(&format!("export declare enum {e} {{")));
        }
        assert!(code(9999).unwrap_err().contains("9999"));
        assert_eq!(
            mouse_button(GAMEPAD_BUTTON_BASE).unwrap_err(),
            format!("{GAMEPAD_BUTTON_BASE} (a GamepadButton) is not a MouseButton; use MouseButton.Left and friends from bevy-ts/input")
        );
        assert!(gamepad_button(MOUSE_BUTTON_BASE).unwrap_err().contains("(a MouseButton)"));
        assert!(gamepad_input(1).unwrap_err().contains("(a KeyCode)"));
    }

    /// `Key`'s named members are the Rust list, numbered from `KEY_BASE`, above every other enum.
    #[test]
    fn the_ts_key_is_the_list() {
        for (i, (name, k)) in NAMED_KEYS.iter().enumerate() {
            let n = KEY_BASE + i as u32;
            assert!(js().contains(&format!("  {name}: {n},\n")));
            assert!(dts().contains(&format!("  const {name}: Key;\n")));
            assert_eq!(key(n).as_ref(), Ok(k));
            assert!(input_code_of(n).is_none());
        }
        assert!(OCTANT_BASE + OCTANTS.len() as u32 <= KEY_BASE);
        assert!(key(0).unwrap_err().contains("(a KeyCode) is not a Key"));
        assert!(mouse_button(KEY_BASE).unwrap_err().contains("(a Key)"));
    }

    /// The declarations type-check, and a misspelt member is a type error. Needs `deno`.
    #[test]
    fn key_declarations_type_check() {
        if std::process::Command::new("deno").arg("--version").output().is_err() {
            eprintln!("skipped: no deno");
            return;
        }
        // Per process: another worktree's run must not share its files.
        let dir = std::env::temp_dir().join(format!("bevy-ts-input-dts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let decls = format!("{KEY_DTS}{}}}\n", NAMED_KEYS.iter().map(|(n, _)| format!("  const {n}: Key;\n")).collect::<String>());
        std::fs::write(dir.join("input.ts"), decls).unwrap();
        let check = r#"import { Key } from "./input.ts";
const enter: Key = Key.Enter;
const z: Key = Key.Character("z");
const dead: Key = Key.Dead();
const n: number = enter + z + dead;
// @ts-expect-error  a misspelt member
Key.Entr;
// @ts-expect-error  a number is not a Key
const bad: Key = 42;
export { bad, n };
"#;
        std::fs::write(dir.join("check.ts"), check).unwrap();
        let out = std::process::Command::new("deno").arg("check").arg("check.ts").current_dir(&dir).output().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }
}
