//! Change `headless-keys`: under `bevy-ts dev --headless`, `bevy-ts/viewport_input` takes `key` and
//! `ime` events onto the window-less primary window, through the mapping `--viewport` uses, so a
//! test types into an `<input>` and submits. One queue with the pointer, one event a frame, in
//! arrival order. Every platform.

use crate::common;
use crate::headless_pointer::{dev_game, pointer, post};

use common::{BOUND, LAUNCH};
use serde_json::{Value, json};

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
  />
));
"#
    )
}

/// A few frames for the camera and the field's layout: each request is answered on a frame after
/// the one before it.
fn settle(port: u16) {
    for _ in 0..10 {
        post(port, "bevy-ts/hello", Value::Null).unwrap();
    }
}

#[test]
fn a_test_types_and_submits_headless() {
    let (_engine, rx, port) = dev_game("headless-keys-submit", "{}", &game("autoFocus"));
    common::wait_for(&rx, &["generation 1 applied"], LAUNCH);
    settle(port);
    let r = post(port, "bevy-ts/viewport_input", json!({ "events": press(&[("KeyH", "h"), ("KeyI", "i"), ("Enter", "Enter")]) })).unwrap();
    assert_eq!(r, json!({ "queued": 6 }));
    // `Keys` reads `ButtonInput<KeyCode>`, filled from the same `KeyboardInput`.
    let lines = common::wait_for(&rx, &["KeyH just pressed", "submit \"hi\""], BOUND);
    std::thread::sleep(std::time::Duration::from_millis(300));
    let submits = lines.iter().chain(rx.try_iter().collect::<Vec<_>>().iter()).filter(|l| l.contains("submit ")).count();
    assert_eq!(submits, 1, "{lines:?}");
}

#[test]
fn clicking_a_field_then_typing_headless() {
    let (_engine, rx, port) = dev_game("headless-keys-click", "{}", &game(""));
    common::wait_for(&rx, &["generation 1 applied"], LAUNCH);
    settle(port);
    // The click and the key in one request: the queue keeps their order, so the key lands after
    // the click has focused the field.
    let (x, y) = (480.0, 270.0);
    let mut events = vec![pointer("move", x, y), pointer("down", x, y), pointer("up", x, y)];
    events.extend(press(&[("KeyA", "a")]));
    let r = post(port, "bevy-ts/viewport_input", json!({ "events": events })).unwrap();
    assert_eq!(r, json!({ "queued": 5 }));
    common::wait_for(&rx, &["change \"a\""], BOUND);
}

#[test]
fn typing_guards_a_shortcut_headless() {
    let (_engine, rx, port) = dev_game("headless-keys-typing", "{}", &game(""));
    common::wait_for(&rx, &["generation 1 applied"], LAUNCH);
    settle(port);
    // Bevy focuses the primary window at start: N counts.
    post(port, "bevy-ts/viewport_input", json!({ "events": press(&[("KeyN", "n")]) })).unwrap();
    common::wait_for(&rx, &["n focused true typing false count 1"], BOUND);
    // A click on the field focuses it: N is typed, not counted.
    let (x, y) = (480.0, 270.0);
    let mut events = vec![pointer("move", x, y), pointer("down", x, y), pointer("up", x, y)];
    events.extend(press(&[("KeyN", "n")]));
    post(port, "bevy-ts/viewport_input", json!({ "events": events })).unwrap();
    common::wait_for(&rx, &["n focused true typing true count 1"], BOUND);
}
