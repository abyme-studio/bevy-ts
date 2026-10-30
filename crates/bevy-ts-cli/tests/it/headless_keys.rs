//! Change `headless-keys`: under `bevy-ts dev --headless`, `bevy-ts/viewport_input` takes `key` and
//! `ime` events onto the window-less primary window, through the mapping `--viewport` uses, so a
//! test types into an `<input>` and submits. One queue with the pointer, one event a frame, in
//! arrival order. Every platform.

use crate::common;
use crate::headless_pointer::{dev_game, pointer, post};

use common::{BOUND, LAUNCH};
use serde_json::{Value, json};
use std::sync::mpsc::Receiver;

fn key(state: &str, code: &str, key: &str) -> Value {
    let text = (state == "down" && key.chars().count() == 1).then_some(key);
    json!({ "type": "key", "state": state, "code": code, "key": key, "text": text })
}

/// Down and up for each `(code, key)`.
fn press(keys: &[(&str, &str)]) -> Vec<Value> {
    keys.iter().flat_map(|(c, k)| [key("down", c, k), key("up", c, k)]).collect()
}

fn game(props: &str) -> String {
    format!(
        r#"import {{ app, px, Startup, Update }} from "bevy-ts";
import type {{ Commands }} from "bevy-ts";
import {{ Camera3d, Transform }} from "bevy-ts/bevy";
import {{ KeyCode, type InputFocus, type Keys }} from "bevy-ts/input";

export function camera(commands: Commands) {{
  commands.spawn(Camera3d, Transform.lookingAt([0, 0, 4], [0, 0, 0]));
}}
app.addSystems(Startup, camera);

let count = 0;
export function watch(keys: Keys, focus: InputFocus) {{
  if (keys.justPressed(KeyCode.KeyH)) console.log("KeyH just pressed");
  if (keys.justPressed(KeyCode.KeyN)) {{
    if (!focus.typing()) count += 1;
    console.log(`n focused ${{focus.get() !== undefined}} typing ${{focus.typing()}} count ${{count}}`);
  }}
}}
app.addSystems(Update, watch);

app.scene("form", () => (
  <input
    key="field"
    {props}
    style={{{{ width: px(960), height: px(540) }}}}
    onTextEditChange={{(e) => console.log(`change ${{JSON.stringify(e.value)}}`)}}
    onSubmit={{(e) => console.log(`submit ${{JSON.stringify(e.value)}}`)}}
    onFocusGained={{(e) => console.log(`field focused ${{e.cause}}`)}}
    onPointerOver={{() => console.log("field over")}}
  />
));
"#
    )
}

/// Moves the pointer onto the field and waits for its over: the camera and the field's layout are
/// up, however many frames a loaded machine takes, since a resting pointer is cast again each frame.
fn hover(port: u16, rx: &Receiver<String>, x: f64, y: f64) {
    let r = post(port, "bevy-ts/viewport_input", json!({ "events": [pointer("move", x, y)] })).unwrap();
    assert_eq!(r, json!({ "queued": 1 }));
    common::wait_for(rx, &["field over"], BOUND);
}

#[test]
fn a_test_types_and_submits_headless() {
    let (_dir, _engine, rx, port) = dev_game("headless-keys-submit", "{}", &game("autoFocus"));
    // Keys go to the focused field: wait for autoFocus to focus it.
    common::wait_for(&rx, &["generation 1 applied", "field focused auto"], LAUNCH);
    let r = post(port, "bevy-ts/viewport_input", json!({ "events": press(&[("KeyH", "h"), ("KeyI", "i"), ("Enter", "Enter")]) })).unwrap();
    assert_eq!(r, json!({ "queued": 6 }));
    // `Keys` reads `ButtonInput<KeyCode>`, filled from the same `KeyboardInput`.
    let mut lines = common::wait_for(&rx, &["KeyH just pressed", "submit \"hi\""], BOUND);
    // N comes a frame after Enter's up, so every line before its own counts.
    post(port, "bevy-ts/viewport_input", json!({ "events": press(&[("KeyN", "n")]) })).unwrap();
    common::wait_more(&rx, &mut lines, &["n focused"], BOUND);
    assert_eq!(lines.iter().filter(|l| l.contains("submit ")).count(), 1, "{lines:?}");
}

#[test]
fn clicking_a_field_then_typing_headless() {
    let (_dir, _engine, rx, port) = dev_game("headless-keys-click", "{}", &game(""));
    common::wait_for(&rx, &["generation 1 applied"], LAUNCH);
    let (x, y) = (480.0, 270.0);
    hover(port, &rx, x, y);
    // The click and the key in one request: the queue keeps their order, so the key lands after
    // the click has focused the field.
    let mut events = vec![pointer("down", x, y), pointer("up", x, y)];
    events.extend(press(&[("KeyA", "a")]));
    let r = post(port, "bevy-ts/viewport_input", json!({ "events": events })).unwrap();
    assert_eq!(r, json!({ "queued": 4 }));
    common::wait_for(&rx, &["change \"a\""], BOUND);
}

#[test]
fn typing_guards_a_shortcut_headless() {
    let (_dir, _engine, rx, port) = dev_game("headless-keys-typing", "{}", &game(""));
    common::wait_for(&rx, &["generation 1 applied"], LAUNCH);
    // Bevy focuses the primary window at start: N counts.
    post(port, "bevy-ts/viewport_input", json!({ "events": press(&[("KeyN", "n")]) })).unwrap();
    common::wait_for(&rx, &["n focused true typing false count 1"], BOUND);
    // A click on the field focuses it: N is typed, not counted.
    let (x, y) = (480.0, 270.0);
    hover(port, &rx, x, y);
    let mut events = vec![pointer("down", x, y), pointer("up", x, y)];
    events.extend(press(&[("KeyN", "n")]));
    post(port, "bevy-ts/viewport_input", json!({ "events": events })).unwrap();
    common::wait_for(&rx, &["n focused true typing true count 1"], BOUND);
}
