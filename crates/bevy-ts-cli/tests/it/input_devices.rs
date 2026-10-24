//! Change `script-input-devices`: `MouseButtons`, `Cursor`, `Gamepads` and `Touches` from
//! `bevy-ts/input`, read by TS systems in a headless app. Each test writes the messages Bevy's
//! input systems consume, runs a frame, and reads what the system logged.

use crate::common;

use bevy::input::ButtonState;
use bevy::input::InputPlugin;
use bevy::input::gamepad::{
    Gamepad, GamepadConnection, GamepadConnectionEvent, RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent,
    RawGamepadEvent,
};
use bevy::input::mouse::MouseButtonInput;
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::window::{ExitCondition, PrimaryWindow};
use common::messages;

/// A headless app with Bevy's input and window plugins and `bevy-ts/input`, `game.ts` loaded.
fn input_app(name: &str, src: &str, window: bool) -> App {
    let dir = common::project(name, &[("game.ts", src)]);
    let mut app = common::app(&dir.join("game.ts"));
    app.add_plugins(InputPlugin)
        .add_plugins(WindowPlugin {
            primary_window: window.then(Window::default),
            exit_condition: ExitCondition::DontExit,
            ..default()
        })
        .add_plugins(bevy_ts_cli::input::ScriptInputPlugin);
    let r = common::reload(&mut app, false);
    assert!(r.errors.is_empty(), "load failed: {:?}", r.errors);
    app
}

fn window(app: &mut App) -> Entity {
    let world = app.world_mut();
    world.query_filtered::<Entity, With<PrimaryWindow>>().single(world).unwrap()
}

/// The last line the scripts logged starting with `prefix`.
fn last(app: &App, prefix: &str) -> String {
    messages(app).into_iter().rfind(|m| m.starts_with(prefix)).unwrap_or_else(|| panic!("no {prefix:?} in {:?}", messages(app)))
}

fn frame(app: &mut App, prefix: &str) -> String {
    app.update();
    last(app, prefix)
}

const MOUSE: &str = r#"import { app, Update } from "bevy-ts";
import { GamepadButton, MouseButton, type MouseButtons } from "bevy-ts/input";

let tried = false;
export function click(mouse: MouseButtons) {
  const l = MouseButton.Left;
  console.log(`mouse ${mouse.justPressed(l)} ${mouse.pressed(l)} ${mouse.justReleased(l)} ${mouse.pressed(MouseButton.Right)}`);
  if (tried) return;
  tried = true;
  try {
    mouse.pressed(GamepadButton.South);
  } catch (e) {
    console.log(`threw ${(e as Error).message}`);
  }
}
app.addSystems(Update, click);
"#;

#[test]
fn mouse_buttons_read_bevys_button_input() {
    let mut app = input_app("input-mouse", MOUSE, true);
    let w = window(&mut app);
    let send = |app: &mut App, state| {
        app.world_mut().write_message(MouseButtonInput { button: MouseButton::Left, state, window: w });
    };
    assert_eq!(frame(&mut app, "mouse"), "mouse false false false false");
    send(&mut app, ButtonState::Pressed);
    assert_eq!(frame(&mut app, "mouse"), "mouse true true false false", "a click is just pressed that frame");
    assert_eq!(frame(&mut app, "mouse"), "mouse false true false false", "and held after");
    send(&mut app, ButtonState::Released);
    assert_eq!(frame(&mut app, "mouse"), "mouse false false true false");
    assert_eq!(frame(&mut app, "mouse"), "mouse false false false false");

    let threw = last(&app, "threw");
    for needle in ["MouseButtons", "pressed", "8192", "GamepadButton"] {
        assert!(threw.contains(needle), "{needle} in {threw}");
    }
}

const CURSOR: &str = r#"import { app, Update } from "bevy-ts";
import type { Cursor } from "bevy-ts/input";

const at = (v: { x: number; y: number } | undefined) => (v ? `${v.x},${v.y}` : "none");
export function track(cursor: Cursor) {
  console.log(`cursor ${at(cursor.position())} ${at(cursor.physicalPosition())}`);
}
app.addSystems(Update, track);
"#;

#[test]
fn the_cursor_is_the_primary_windows() {
    let mut app = input_app("input-cursor", CURSOR, true);
    assert_eq!(frame(&mut app, "cursor"), "cursor none none", "outside the window");
    let w = window(&mut app);
    app.world_mut().get_mut::<Window>(w).unwrap().set_cursor_position(Some(Vec2::new(100.0, 50.0)));
    assert_eq!(frame(&mut app, "cursor"), "cursor 100,50 100,50");
}

#[test]
fn no_window_reads_as_no_cursor() {
    let mut app = input_app("input-cursor-none", CURSOR, false);
    assert_eq!(frame(&mut app, "cursor"), "cursor none none");
}

const PADS: &str = r#"import { app, Update } from "bevy-ts";
import type { Entity, Query, With } from "bevy-ts";
import { Gamepad, PrimaryWindow } from "bevy-ts/bevy";
import { GamepadAxis, GamepadButton, type Gamepads } from "bevy-ts/input";

let tried = false;
export function pads(gamepads: Gamepads, q: Query<[Entity], With<Gamepad>>, windows: Query<[Entity], With<PrimaryWindow>>) {
  q.each((pad) => {
    const s = GamepadButton.South;
    console.log(
      `pad ${gamepads.justPressed(pad, s)} ${gamepads.pressed(pad, s)} ${gamepads.get(pad, s)} ` +
        `${gamepads.leftStick(pad).x} ${gamepads.get(pad, GamepadAxis.LeftStickX)} ${gamepads.getUnclamped(pad, GamepadAxis.RightStickY)} ` +
        `${gamepads.dpad(pad).x} ${gamepads.vendorId(pad)} ${gamepads.productId(pad)}`,
    );
  });
  if (tried) return;
  windows.each((w) => {
    tried = true;
    try {
      gamepads.pressed(w, GamepadButton.South);
    } catch (e) {
      console.log(`threw ${(e as Error).message} window ${w}`);
    }
  });
}
app.addSystems(Update, pads);
"#;

#[test]
fn gamepads_read_each_pads_state() {
    let mut app = input_app("input-gamepads", PADS, true);
    let pad = app.world_mut().spawn_empty().id();
    let connected = GamepadConnection::Connected { name: "Test pad".into(), vendor_id: Some(0x45e), product_id: None };
    app.world_mut().write_message(RawGamepadEvent::Connection(GamepadConnectionEvent::new(pad, connected)));
    common::until(&mut app, "the pad connects", |app| app.world().get::<Gamepad>(pad).is_some());
    app.update();
    assert_eq!(last(&app, "pad"), "pad false false undefined 0 undefined undefined 0 1118 undefined");

    let south = RawGamepadButtonChangedEvent::new(pad, GamepadButton::South, 1.0);
    let stick = RawGamepadAxisChangedEvent::new(pad, GamepadAxis::LeftStickX, 0.5);
    app.world_mut().write_message(RawGamepadEvent::Button(south));
    app.world_mut().write_message(RawGamepadEvent::Axis(stick));
    let line = frame(&mut app, "pad");
    let x = app.world().get::<Gamepad>(pad).unwrap().left_stick().x;
    assert_eq!(line, format!("pad true true 1 {x} {x} undefined 0 1118 undefined"), "Bevy's processed value");
    assert!(frame(&mut app, "pad").starts_with("pad false true 1 "), "just pressed is one frame");

    let threw = last(&app, "threw");
    let w = threw.rsplit(' ').next().unwrap();
    assert!(threw.contains("Gamepads") && threw.contains("pressed") && threw.contains(&format!("entity {w} is not a gamepad")), "{threw}");
}

const TOUCH: &str = r#"import { app, Update } from "bevy-ts";
import type { Touches } from "bevy-ts/input";

const v = (p: { x: number; y: number } | undefined) => (p ? `${p.x},${p.y}` : "none");
export function fingers(touches: Touches) {
  const first = touches.justPressedAt(0);
  const one = touches.getPressed(1);
  console.log(
    `touch ${touches.anyJustPressed()} ${first?.id} ${touches.justPressedCount()} ${touches.count()} ` +
      `${v(one?.position)} ${v(one?.delta)} ${v(one?.startPosition)} ${v(touches.firstPressedPosition())} ` +
      `${touches.anyJustReleased()} ${touches.justReleasedAt(0)?.id} ${v(touches.getReleased(1)?.distance)} ${touches.at(1)?.id}`,
  );
}
app.addSystems(Update, fingers);
"#;

#[test]
fn touches_list_by_index_with_bevys_positions() {
    let mut app = input_app("input-touches", TOUCH, true);
    let w = window(&mut app);
    let touch = |app: &mut App, phase, x: f32| {
        app.world_mut().write_message(TouchInput { phase, position: Vec2::new(x, 10.0), window: w, force: None, id: 1 });
    };
    assert_eq!(frame(&mut app, "touch"), "touch false undefined 0 0 none none none none false undefined none undefined");
    touch(&mut app, TouchPhase::Started, 10.0);
    assert_eq!(frame(&mut app, "touch"), "touch true 1 1 1 10,10 0,0 10,10 10,10 false undefined none undefined");
    touch(&mut app, TouchPhase::Moved, 20.0);
    assert_eq!(frame(&mut app, "touch"), "touch false undefined 0 1 20,10 10,0 10,10 20,10 false undefined none undefined");
    touch(&mut app, TouchPhase::Ended, 20.0);
    assert_eq!(frame(&mut app, "touch"), "touch false undefined 0 0 none none none none true 1 10,0 undefined");
}
