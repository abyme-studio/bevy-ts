//! Change `text-input`: `<input>` typed into through `bevy-ts/viewport_input`, and `bevy-ts/input`
//! read by a TS system. Viewport mode, as `bevy-ts dev --viewport` builds it: Bevy dispatches keys
//! to `InputFocus` only with a primary window, and `--headless` has none.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::text::{EditableText, LineBreak, TextCursorStyle};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_v8::V8Options;
use common::{messages, project, reload, write};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn viewport_app(entry: &Path) -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(RemotePlugin::default())
    .add_plugins(bevy_ts_viewport::ViewportPlugin { windowed: false })
    .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin)
    .add_plugins(bevy_ts_cli::input::ScriptInputPlugin);
    app.insert_resource(DevSource(Arc::new(Compiler::new(entry, true).unwrap())));
    bevy_ts_remote::add_methods(&mut app);
    app.finish();
    app.cleanup();
    app
}

fn call(app: &mut App, method: &str, params: Value) -> Value {
    let id = match app.world().resource::<RemoteMethods>().get(method) {
        Some(RemoteMethodSystemId::Instant(id)) => *id,
        _ => panic!("{method} not served"),
    };
    app.world_mut().run_system_with(id, Some(params)).unwrap().unwrap_or_else(|e| panic!("{method}: {}", e.message))
}

fn updates(app: &mut App, n: usize) {
    for _ in 0..n {
        std::thread::sleep(Duration::from_millis(5));
        app.update();
    }
}

/// A key down as a viewer sends it: `text` only for a key that types.
fn down(code: &str, key: &str) -> Value {
    let text = (key.chars().count() == 1).then_some(key);
    json!({ "type": "key", "state": "down", "code": code, "key": key, "text": text })
}

fn up(code: &str, key: &str) -> Value {
    json!({ "type": "key", "state": "up", "code": code, "key": key })
}

fn input(app: &mut App, events: Vec<Value>) {
    call(app, "bevy-ts/viewport_input", json!({ "events": events }));
}

/// The app with `game.tsx` loaded and a viewer attached, so the virtual window exists.
fn start(name: &str, game: &str) -> (std::path::PathBuf, App) {
    let dir = project(name, &[("game.tsx", game)]);
    let mut app = viewport_app(&dir.join("game.tsx"));
    let report = reload(&mut app, false);
    assert_eq!(report.status, ReloadStatus::Applied, "{:?}", messages(&app));
    call(&mut app, "bevy-ts/viewport_attach", json!({ "w": 400, "h": 300, "scale": 1 }));
    updates(&mut app, 3);
    (dir, app)
}

fn field(app: &mut App) -> Entity {
    let world = app.world_mut();
    world.query_filtered::<Entity, With<EditableText>>().single(world).unwrap()
}

fn value(app: &mut App) -> String {
    let e = field(app);
    app.world().get::<EditableText>(e).unwrap().value().to_string()
}

fn focus(app: &App) -> Option<Entity> {
    app.world().resource::<InputFocus>().get()
}

/// Messages logged by the scripts, without the engine's own.
fn logged(app: &App, prefix: &str) -> Vec<String> {
    messages(app).into_iter().filter(|m| m.starts_with(prefix)).collect()
}

const FIELD: &str = r#"import { app } from "bevy-ts";

app.scene("form", () => (
  <input
    key="field"
    autoFocus
    onKeyboardInput={(e) => { if (e.state === "down") console.log(`key ${e.key} ${e.code} ${JSON.stringify(e.value)}`); }}
    onSubmit={(e) => console.log(`submit ${JSON.stringify(e.value)} ${e.frame > 0}`)}
    onCancel={(e) => console.log(`cancel ${e.entity === e.entity}`)}
  />
));
"#;

#[test]
fn typed_keys_then_enter_submit_the_text() {
    let (_dir, mut app) = start("text-input-frames", FIELD);
    let f = field(&mut app);
    assert_eq!(focus(&app), Some(f), "autoFocus");
    for (code, key) in [("KeyH", "h"), ("KeyI", "i"), ("Enter", "Enter")] {
        input(&mut app, vec![down(code, key)]);
        updates(&mut app, 2);
        input(&mut app, vec![up(code, key)]);
        updates(&mut app, 1);
    }
    assert_eq!(logged(&app, "key "), vec![r#"key h KeyH "h""#, r#"key i KeyI "hi""#, r#"key Enter Enter "hi""#]);
    assert_eq!(logged(&app, "submit"), vec![r#"submit "hi" true"#]);
    assert_eq!(value(&mut app), "hi");
}

#[test]
fn three_keys_in_one_frame_still_submit_the_text() {
    let (_dir, mut app) = start("text-input-one-frame", FIELD);
    input(&mut app, vec![down("KeyH", "h"), down("KeyI", "i"), down("Enter", "Enter")]);
    updates(&mut app, 2);
    assert_eq!(logged(&app, "key Enter"), vec![r#"key Enter Enter "hi""#]);
    assert_eq!(logged(&app, "submit"), vec![r#"submit "hi" true"#]);
}

#[test]
fn a_held_enter_submits_once() {
    let (_dir, mut app) = start("text-input-held-enter", FIELD);
    input(&mut app, vec![down("KeyA", "a")]);
    updates(&mut app, 2);
    input(&mut app, vec![down("Enter", "Enter")]);
    updates(&mut app, 2);
    for _ in 0..3 {
        input(&mut app, vec![json!({ "type": "key", "state": "down", "code": "Enter", "key": "Enter", "repeat": true })]);
        updates(&mut app, 1);
    }
    updates(&mut app, 2);
    assert_eq!(logged(&app, "key Enter").len(), 4, "onKeyboardInput runs for each repeat");
    assert_eq!(logged(&app, "submit"), vec![r#"submit "a" true"#]);
}

#[test]
fn escape_cancels_and_clears_focus() {
    let (_dir, mut app) = start("text-input-escape", FIELD);
    input(&mut app, vec![down("KeyA", "a")]);
    updates(&mut app, 2);
    input(&mut app, vec![down("Escape", "Escape")]);
    updates(&mut app, 2);
    assert_eq!(logged(&app, "key Escape"), vec![r#"key Escape Escape "a""#]);
    assert_eq!(logged(&app, "cancel"), vec!["cancel true"]);
    assert_eq!(logged(&app, "submit"), Vec::<String>::new());
    assert_eq!(focus(&app), None);
    // With no focus, keys reach no field.
    input(&mut app, vec![down("KeyB", "b")]);
    updates(&mut app, 2);
    assert_eq!(value(&mut app), "a");
}

fn limited(initial: &str) -> String {
    format!(
        r#"import {{ app }} from "bevy-ts";

app.scene("form", () => (
  <input key="field" autoFocus initialValue="{initial}" maxCharacters={{3}} onTextEditChange={{(e) => console.log(`change ${{JSON.stringify(e.value)}}`)}} />
));
"#
    )
}

#[test]
fn initial_value_max_characters_change_events_and_a_reload() {
    let (dir, mut app) = start("text-input-props", &limited("x"));
    let f = field(&mut app);
    assert_eq!(value(&mut app), "x");
    // One line that never wraps, and a cursor Bevy draws, in the text's colour.
    assert_eq!(app.world().get::<TextLayout>(f).unwrap().linebreak, LineBreak::NoWrap);
    assert_eq!(app.world().get::<TextCursorStyle>(f).unwrap().color, app.world().get::<TextColor>(f).unwrap().0);
    for (code, key) in [("KeyA", "a"), ("KeyB", "b"), ("KeyC", "c")] {
        input(&mut app, vec![down(code, key)]);
        updates(&mut app, 3);
    }
    assert_eq!(value(&mut app), "xab", "maxCharacters drops the fourth character");
    assert!(logged(&app, "change").contains(&r#"change "xa""#.to_string()), "{:?}", logged(&app, "change"));
    // A changed initialValue does not reset the field; its entity is kept.
    write(&dir, "game.tsx", &limited("y"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    updates(&mut app, 2);
    assert_eq!(field(&mut app), f);
    assert_eq!(value(&mut app), "xab");
}

const COUNTER: &str = r#"import { app, Update } from "bevy-ts";
import { KeyCode, type InputFocus, type Keys } from "bevy-ts/input";

let count = 0;
export function countN(keys: Keys, focus: InputFocus) {
  if (keys.justPressed(KeyCode.KeyN)) console.log(`n ${focus.typing() ? "field" : "free"}`);
  if (keys.justPressed(KeyCode.KeyN) && !focus.typing()) {
    count += 1;
    console.log(`count ${count}`);
  }
}
app.addSystems(Update, countN);

app.scene("form", () => <input key="field" autoFocus />);
"#;

#[test]
fn typing_guards_a_shortcut_only_while_a_field_has_focus() {
    let src = COUNTER.replace(" autoFocus", "");
    let (_dir, mut app) = start("text-input-typing", &src);
    let press = |app: &mut App| {
        input(app, vec![down("KeyN", "n")]);
        updates(app, 2);
        input(app, vec![up("KeyN", "n")]);
        updates(app, 2);
    };
    let f = field(&mut app);
    assert_ne!(focus(&app), Some(f));
    press(&mut app);
    assert_eq!(logged(&app, "count"), vec!["count 1"], "no field has focus: {:?}", focus(&app));
    app.world_mut().resource_mut::<InputFocus>().set(f, bevy::input_focus::FocusCause::Navigated);
    press(&mut app);
    assert_eq!(logged(&app, "n "), vec!["n free", "n field"]);
    assert_eq!(logged(&app, "count"), vec!["count 1"]);
}

#[test]
fn keys_and_input_focus_read_n_only_while_nothing_has_focus() {
    let (_dir, mut app) = start("text-input-keys", COUNTER);
    let press = |app: &mut App, code: &str, key: &str| {
        input(app, vec![down(code, key)]);
        updates(app, 2);
        input(app, vec![up(code, key)]);
        updates(app, 2);
    };
    press(&mut app, "KeyN", "n");
    assert_eq!(logged(&app, "n "), vec!["n field"], "Keys sees the key, as Bevy's ButtonInput does");
    assert_eq!(logged(&app, "count"), Vec::<String>::new());
    press(&mut app, "Escape", "Escape");
    assert_eq!(focus(&app), None);
    press(&mut app, "KeyN", "n");
    assert_eq!(logged(&app, "count"), vec!["count 1"]);
    updates(&mut app, 3);
    assert_eq!(logged(&app, "count"), vec!["count 1"], "justPressed is one frame");
}

#[test]
fn an_unknown_key_code_throws_naming_the_param_method_and_number() {
    let src = r#"import { app, Update } from "bevy-ts";
import type { Keys } from "bevy-ts/input";

let done = false;
export function bad(keys: Keys) {
  if (done) return;
  done = true;
  try {
    keys.pressed(9999);
  } catch (e) {
    console.log(`threw ${(e as Error).message}`);
  }
}
app.addSystems(Update, bad);
"#;
    let (_dir, app) = start("text-input-bad-code", src);
    let threw = logged(&app, "threw");
    assert_eq!(threw.len(), 1, "{:?}", messages(&app));
    for needle in ["Keys", "pressed", "9999"] {
        assert!(threw[0].contains(needle), "{needle} in {}", threw[0]);
    }
}

#[test]
fn typed_text_arrives_once_from_a_key_or_from_an_ime_commit() {
    let (_dir, mut app) = start("text-input-ime", FIELD);
    // Plain typing: a key with text, no IME event.
    input(&mut app, vec![down("KeyH", "h")]);
    updates(&mut app, 2);
    assert_eq!(value(&mut app), "h");
    // A composition: preedit then commit, its keys carrying no text.
    input(&mut app, vec![
        json!({ "type": "key", "state": "down", "code": "KeyE", "key": "Process" }),
        json!({ "type": "ime", "kind": "preedit", "text": "e", "cursor": [1, 1] }),
    ]);
    updates(&mut app, 2);
    input(&mut app, vec![json!({ "type": "ime", "kind": "commit", "text": "é" })]);
    updates(&mut app, 2);
    assert_eq!(value(&mut app), "hé");
    // Option held, as a native window sends it: the Alt key, then the key with its text. Bevy's
    // TextInput inserts text only with no modifier or Shift.
    input(&mut app, vec![
        json!({ "type": "key", "state": "down", "code": "AltLeft", "key": "Alt", "mods": { "alt": true } }),
        json!({ "type": "key", "state": "down", "code": "KeyE", "key": "´", "text": "´", "mods": { "alt": true } }),
    ]);
    updates(&mut app, 2);
    assert_eq!(value(&mut app), "hé");
}

// Change `logical-keys`: `LogicalKeys` and `Key` from `bevy-ts/input`, read from the `key` the
// viewer sends, as Bevy's `ButtonInput<Key>` fills from each `KeyboardInput`'s `logical_key`.
const LOGICAL: &str = r#"import { app, Update } from "bevy-ts";
import { Key, KeyCode, type Keys, type LogicalKeys } from "bevy-ts/input";

let held = false;
export function watch(keys: LogicalKeys, codes: Keys) {
  if (keys.justPressed(Key.Character("z"))) console.log(`z KeyZ ${codes.justPressed(KeyCode.KeyZ)} KeyY ${codes.justPressed(KeyCode.KeyY)}`);
  if (keys.justPressed(Key.Enter)) console.log("enter down");
  if (keys.pressed(Key.Enter) !== held) {
    held = !held;
    console.log(`enter held ${held}`);
  }
  if (keys.justReleased(Key.Enter)) console.log("enter up");
  if (keys.justPressed(Key.Character("N"))) console.log(`N ${keys.pressed(Key.Character("N"))} n ${keys.pressed(Key.Character("n"))}`);
}
app.addSystems(Update, watch);
"#;

#[test]
fn logical_keys_read_what_the_key_types() {
    let (_dir, mut app) = start("logical-keys", LOGICAL);
    // A German layout: the key at KeyY types z.
    input(&mut app, vec![down("KeyY", "z")]);
    updates(&mut app, 2);
    input(&mut app, vec![up("KeyY", "z")]);
    updates(&mut app, 2);
    assert_eq!(logged(&app, "z "), vec!["z KeyZ false KeyY true"]);

    input(&mut app, vec![down("Enter", "Enter")]);
    updates(&mut app, 3);
    assert_eq!(logged(&app, "enter"), vec!["enter down", "enter held true"]);
    input(&mut app, vec![up("Enter", "Enter")]);
    updates(&mut app, 2);
    assert_eq!(logged(&app, "enter"), vec!["enter down", "enter held true", "enter held false", "enter up"]);

    // Shift held: the key types N, so N is down and n is not.
    input(&mut app, vec![down("ShiftLeft", "Shift"), down("KeyN", "N")]);
    updates(&mut app, 2);
    assert_eq!(logged(&app, "N "), vec!["N true n false"]);
}

#[test]
fn key_character_takes_one_code_point_and_logical_keys_refuse_a_key_code() {
    let src = r#"import { app, Update } from "bevy-ts";
import { Key, KeyCode, type LogicalKeys } from "bevy-ts/input";

let done = false;
export function once(keys: LogicalKeys) {
  if (done) return;
  done = true;
  try {
    Key.Character("ab");
  } catch (e) {
    console.log(`threw ${(e as Error).message}`);
  }
  console.log(`same ${Key.Character("é") === Key.Character("é")}`);
  try {
    keys.pressed(KeyCode.KeyN);
  } catch (e) {
    console.log(`threw ${(e as Error).message}`);
  }
}
app.addSystems(Update, once);
"#;
    let (_dir, app) = start("logical-keys-bad", src);
    let threw = logged(&app, "threw");
    assert_eq!(threw.len(), 2, "{:?}", messages(&app));
    for needle in ["Key.Character", "\"ab\""] {
        assert!(threw[0].contains(needle), "{needle} in {}", threw[0]);
    }
    let n = bevy_ts::keys::KEY_CODES.iter().position(|(name, _)| *name == "KeyN").unwrap().to_string();
    for needle in ["LogicalKeys", "pressed", n.as_str()] {
        assert!(threw[1].contains(needle), "{needle} in {}", threw[1]);
    }
    assert_eq!(logged(&app, "same"), vec!["same true"]);
}
