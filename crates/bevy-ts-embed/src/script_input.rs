//! `bevy-ts/input`, a built-in module every engine ships: `Keys` and `InputFocus`, exported
//! params over Bevy's own `ButtonInput<KeyCode>` and `InputFocus`, and `KeyCode`, a TS enum
//! generated from `bevy_ts::keys`. A TS system reads keys as a Rust one does:
//! `keys.justPressed(KeyCode.KeyN) && focus.get() === undefined`.

use bevy::ecs::system::SystemParam;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_ts::keys::{KEY_CODES, key_code_of};
use bevy_ts_native::prelude::*;
use std::fmt::Write as _;

pub const MODULE: &str = "bevy-ts/input";

/// Registers the module and its two params.
pub struct ScriptInputPlugin;

impl Plugin for ScriptInputPlugin {
    fn build(&self, app: &mut App) {
        app.ts_param::<KeysParam<'static>>()
            .ts_param::<FocusParam<'static>>()
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

/// The module's JS: `KeyCode`, one number per variant.
fn js() -> String {
    let mut s = String::from("// `bevy-ts/input`: generated from bevy_ts::keys.\nexport const KeyCode = Object.freeze({\n");
    for (i, (name, _)) in KEY_CODES.iter().enumerate() {
        let _ = writeln!(s, "  {name}: {i},");
    }
    s.push_str("});\n");
    s
}

/// The module's declarations; `Keys` and `InputFocus` are declared from their `#[export]`.
fn dts() -> String {
    let mut s = String::from("/** Bevy's `KeyCode`, the physical key, by its Rust name. */\nexport declare enum KeyCode {\n");
    for (i, (name, _)) in KEY_CODES.iter().enumerate() {
        let _ = writeln!(s, "  {name} = {i},");
    }
    s.push_str("}\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The TS enum is the Rust list: same names, same numbers.
    #[test]
    fn the_ts_enum_is_the_list() {
        for (i, (name, k)) in KEY_CODES.iter().enumerate() {
            assert!(js().contains(&format!("  {name}: {i},\n")));
            assert!(dts().contains(&format!("  {name} = {i},\n")));
            assert_eq!(code(i as u32), Ok(*k));
        }
        assert!(code(9999).unwrap_err().contains("9999"));
    }
}
