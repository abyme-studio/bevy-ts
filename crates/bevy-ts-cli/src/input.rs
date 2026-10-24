//! `bevy-ts/input`, a built-in module every engine ships: `Keys`, `LogicalKeys` and `InputFocus`,
//! exported params over Bevy's own `ButtonInput<KeyCode>`, `ButtonInput<Key>` and `InputFocus`;
//! `KeyCode`, a TS enum, and `Key`, Bevy's logical key as one number, both generated from
//! `bevy_ts::keys`. A TS system reads keys as a Rust one does:
//! `keys.justPressed(KeyCode.KeyN) && focus.get() === undefined`.

use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::Key;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_ts::keys::{CHARACTER_BASE, DEAD_KEY_BASE, DEAD_KEY_NONE, KEY_BASE, KEY_CODES, NAMED_KEYS, key_code_of, key_of};
use bevy_ts_native::prelude::*;
use std::fmt::Write as _;

pub const MODULE: &str = "bevy-ts/input";

/// Registers the module and its three params.
pub struct ScriptInputPlugin;

impl Plugin for ScriptInputPlugin {
    fn build(&self, app: &mut App) {
        app.ts_param::<KeysParam<'static>>()
            .ts_param::<LogicalKeysParam<'static>>()
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

/// `LogicalKeys`: `Res<ButtonInput<Key>>`, the key by what it types. No resource reads as no key
/// down.
#[derive(SystemParam)]
pub struct LogicalKeysParam<'w> {
    keys: Option<Res<'w, ButtonInput<Key>>>,
}

fn key(n: u32) -> Result<Key, String> {
    key_of(n).ok_or_else(|| format!("{n} is not a Key; use Key.Enter, Key.Character(\"z\") and friends from bevy-ts/input"))
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

/// The module's JS: `KeyCode`, one number per variant, and `Key`: a number per named key, and
/// `Key.Character` and `Key.Dead`, which turn one character into its number here, so no string
/// reaches Rust.
fn js() -> String {
    let mut s = String::from("// `bevy-ts/input`: generated from bevy_ts::keys.\nexport const KeyCode = Object.freeze({\n");
    for (i, (name, _)) in KEY_CODES.iter().enumerate() {
        let _ = writeln!(s, "  {name}: {i},");
    }
    s.push_str("});\n");
    let _ = write!(
        s,
        r#"function codePoint(fn, c) {{
  const cps = typeof c === "string" ? [...c.normalize("NFC")] : [];
  if (cps.length !== 1) throw new RangeError(`Key.${{fn}}(${{JSON.stringify(c)}}): takes one character, one code point after NFC`);
  return cps[0].codePointAt(0);
}}
export const Key = Object.freeze({{
  Character: (c) => {CHARACTER_BASE} + codePoint("Character", c),
  Dead: (c) => (c === undefined ? {DEAD_KEY_NONE} : {DEAD_KEY_BASE} + codePoint("Dead", c)),
"#
    );
    for (i, (name, _)) in NAMED_KEYS.iter().enumerate() {
        let _ = writeln!(s, "  {name}: {},", KEY_BASE + i as u32);
    }
    s.push_str("});\n");
    s
}

/// The module's declarations; `Keys`, `LogicalKeys` and `InputFocus` are declared from their
/// `#[export]`.
fn dts() -> String {
    let mut s = String::from("/** Bevy's `KeyCode`, the physical key, by its Rust name. */\nexport declare enum KeyCode {\n");
    for (i, (name, _)) in KEY_CODES.iter().enumerate() {
        let _ = writeln!(s, "  {name} = {i},");
    }
    s.push_str("}\n");
    s.push_str(
        r#"/** Bevy's `Key`, the logical key: what the key types under the layout and modifiers. */
export type Key = number & { readonly __brand: "Key" };
export declare namespace Key {
  /** `Key::Character(c)`; throws unless `c` is one code point after NFC. */
  function Character(c: string): Key;
  /** `Key::Dead(c)`, or `Key::Dead(None)` with no `c`. */
  function Dead(c?: string): Key;
"#,
    );
    for (name, _) in NAMED_KEYS {
        let _ = writeln!(s, "  const {name}: Key;");
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

    /// `Key`'s named members are the Rust list, numbered from `KEY_BASE`.
    #[test]
    fn the_ts_key_is_the_list() {
        for (i, (name, k)) in NAMED_KEYS.iter().enumerate() {
            let n = KEY_BASE + i as u32;
            assert!(js().contains(&format!("  {name}: {n},\n")));
            assert!(dts().contains(&format!("  const {name}: Key;\n")));
            assert_eq!(key(n).as_ref(), Ok(k));
        }
        assert!(key(0).unwrap_err().contains("is not a Key"));
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
        std::fs::write(dir.join("input.ts"), dts()).unwrap();
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
