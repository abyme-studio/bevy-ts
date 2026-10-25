//! Stage 3 of bevy-ui-coverage: every picking event, `onActivate`, `activateOnPress`,
//! `disabled`, and `NodeRef.has(C)` read by presence in Rust with no JS on quiet frames.

use crate::common;

use bevy::camera::NormalizedRenderTarget;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::touch::TouchPhase;
use bevy::input_focus::FocusedInput;
use bevy::picking::backend::HitData;
use bevy::picking::events::*;
use bevy::picking::pointer::{Location, PointerButton, PointerId};
use bevy::prelude::*;
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::{ActivateOnPress, ButtonPlugin};
use bevy_ts::generation::ReloadStatus;
use bevy_ts_tsx::{TsxNode, TsxTree};
use common::*;
use std::path::PathBuf;

fn setup(name: &str, src: &str) -> (PathBuf, App) {
    let dir = project(name, &[("game.tsx", src)]);
    let mut app = app(&dir.join("game.tsx"));
    app.add_plugins(ButtonPlugin);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    let errors: Vec<_> = logs(&app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message).collect();
    assert!(errors.is_empty(), "{errors:?}");
    (dir, app)
}

fn node(app: &mut App, path: &[&str]) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == path).map(|(e, _)| e).unwrap_or_else(|| panic!("no node {path:?}"))
}

fn text(app: &mut App, path: &[&str]) -> String {
    let e = node(app, path);
    app.world().get::<Text>(e).map(|t| t.0.clone()).unwrap_or_default()
}

fn calls(app: &App) -> u64 {
    app.world().resource::<TsxTree>().reactive.calls
}

fn pointer() -> Pointer {
    Pointer::new(PointerId::Mouse, Location { target: NormalizedRenderTarget::None { width: 1, height: 1 }, position: Vec2::new(3.0, 4.0) })
}

fn hit() -> HitData {
    HitData::new(Entity::PLACEHOLDER, 0.0, None, None)
}

const EVENTS: &str = r##"import { app, signal } from "bevy-ts";
const [log, setLog] = signal<string[]>([]);
const add = (s: string) => setLog((l) => [...l, s]);
app.scene("ui", () => (
  <node key="root">
    <node key="t"
      onPointerEnter={(e) => add(`${e.kind}`)}
      onPointerLeave={(e) => add(`${e.kind}`)}
      onPointerMove={(e) => add(`${e.kind}:${e.delta}`)}
      onPointerCancel={(e) => add(`${e.kind}`)}
      onPointerDragStart={(e) => add(`${e.kind}:${e.button}`)}
      onPointerDrag={(e) => add(`${e.kind}:${e.delta}:${e.distance}`)}
      onPointerDragEnd={(e) => add(`${e.kind}:${e.distance}`)}
      onPointerDragEnter={(e) => add(`${e.kind}:${e.dragged === e.entity}`)}
      onPointerDragOver={(e) => add(`${e.kind}:${e.dragged === e.entity}`)}
      onPointerDragLeave={(e) => add(`${e.kind}:${e.dragged === e.entity}`)}
      onPointerDragDrop={(e) => add(`${e.kind}:${e.dropped === e.entity}`)}
      onPointerScroll={(e) => add(`${e.kind}:${e.unit}:${e.x}:${e.y}:${e.target === e.entity}`)}
    >
      <node key="child" />
    </node>
    <text key="log">{() => log().join(" ")}</text>
  </node>
));
"##;

#[test]
fn every_picking_event_reaches_its_handler_with_bevys_fields() {
    let _hang = common::watchdog("every_picking_event_reaches_its_handler_with_bevys_fields", BOUND);
    let (_dir, mut app) = setup("ui-events-all", EVENTS);
    let (e, child) = (node(&mut app, &["root", "t"]), node(&mut app, &["root", "t", "child"]));
    let (p, h, b) = (pointer(), hit(), PointerButton::Primary);
    let w = app.world_mut();
    w.trigger(PointerEnter { entity: e, pointer: p.clone(), hit: h.clone(), is_in_bounds: true });
    w.trigger(PointerLeave { entity: e, pointer: p.clone(), hit: h.clone(), was_in_bounds: true });
    w.trigger(PointerMove { entity: e, pointer: p.clone(), hit: h.clone(), delta: Vec2::new(1.0, 2.0) });
    w.trigger(PointerCancel { entity: e, pointer: p.clone(), hit: h.clone() });
    w.trigger(PointerDragStart { entity: e, pointer: p.clone(), button: b, hit: h.clone() });
    w.trigger(PointerDrag { entity: e, pointer: p.clone(), button: b, distance: Vec2::new(5.0, 0.0), delta: Vec2::new(1.0, 0.0) });
    w.trigger(PointerDragEnd { entity: e, pointer: p.clone(), button: b, distance: Vec2::new(6.0, 0.0) });
    w.trigger(PointerDragEnter { entity: e, pointer: p.clone(), button: b, dragged: e, hit: h.clone() });
    w.trigger(PointerDragOver { entity: e, pointer: p.clone(), button: b, dragged: e, hit: h.clone() });
    w.trigger(PointerDragLeave { entity: e, pointer: p.clone(), button: b, dragged: e, hit: h.clone() });
    w.trigger(PointerDragDrop { entity: e, pointer: p.clone(), button: b, dropped: e, hit: h.clone() });
    // A scroll on the child bubbles to the parent's handler, its target the child.
    w.trigger(PointerScroll { entity: child, pointer: p, unit: MouseScrollUnit::Line, x: 0.0, y: -3.0, hit: h, phase: TouchPhase::Moved });
    let before = calls(&app);
    app.update();
    assert_eq!(calls(&app), before + 1, "one host call for every queued event");
    assert_eq!(
        text(&mut app, &["root", "log"]),
        "PointerEnter PointerLeave PointerMove:1,2 PointerCancel PointerDragStart:primary PointerDrag:1,0:5,0 PointerDragEnd:6,0 \
         PointerDragEnter:true PointerDragOver:true PointerDragLeave:true PointerDragDrop:true PointerScroll:line:0:-3:false"
    );
}

const BUTTONS: &str = r##"import { app, Component, ref, signal, u8, Update, type Query } from "bevy-ts";
import { Pressed } from "bevy-ts/bevy";
export class Off extends Component { static path = "game::Off"; declare on: u8 }
const [log, setLog] = signal("");
const [off, setOff] = signal(false);
export function disable(q: Query<[Off]>) {
  q.each(() => { if (!off()) setOff(true); });
}
app.addSystems(Update, disable);
const b = ref();
app.scene("ui", () => (
  <node key="root">
    <button key="go" ref={b} disabled={() => off()} background={() => (b.has(Pressed) ? "#224466" : "#3366cc")} onActivate={({ entity }) => setLog((l) => `${l}go `)} />
    <button key="early" activateOnPress onActivate={() => setLog((l) => `${l}early `)} />
    <text key="log">{() => log()}</text>
  </node>
));
"##;

// Each applies the commands Bevy's button observers queue (`Pressed`, `Activate`), as a frame would
// between the pointer's press, click and release.
fn press(app: &mut App, e: Entity) {
    app.world_mut().trigger(PointerPress { entity: e, pointer: pointer(), button: PointerButton::Primary, hit: hit(), count: 1 });
    app.world_mut().flush();
}

fn release(app: &mut App, e: Entity) {
    app.world_mut().trigger(PointerRelease { entity: e, pointer: pointer(), button: PointerButton::Primary, hit: hit() });
    app.world_mut().flush();
}

fn click(app: &mut App, e: Entity) {
    app.world_mut().trigger(PointerClick { entity: e, pointer: pointer(), button: PointerButton::Primary, hit: hit(), duration: std::time::Duration::ZERO, count: 1 });
    app.world_mut().flush();
}

fn key(app: &mut App, e: Entity, code: KeyCode, logical: Key) {
    // A real Window: FocusedInput bubbles to `window` and stops only at an entity that is one.
    let window = app.world_mut().spawn(Window::default()).id();
    let input = KeyboardInput { key_code: code, logical_key: logical, state: ButtonState::Pressed, text: None, repeat: false, window };
    app.world_mut().trigger(FocusedInput::new(e, input, window));
    app.world_mut().flush();
}

fn background(app: &App, e: Entity) -> Color {
    app.world().get::<BackgroundColor>(e).unwrap().0
}

#[test]
fn has_pressed_styles_a_button_with_one_call_per_change_and_none_when_quiet() {
    let _hang = common::watchdog("has_pressed_styles_a_button_with_one_call_per_change_and_none_when_quiet", BOUND);
    let (_dir, mut app) = setup("ui-events-has", BUTTONS);
    let go = node(&mut app, &["root", "go"]);
    assert_eq!(background(&app, go), Color::from(Srgba::hex("#3366cc").unwrap()));
    let quiet = calls(&app);
    updates(&mut app, 30);
    assert_eq!(calls(&app), quiet, "no host call on quiet frames");
    press(&mut app, go);
    app.update();
    assert!(app.world().get::<Pressed>(go).is_some(), "Bevy marks the button Pressed");
    assert_eq!(background(&app, go), Color::from(Srgba::hex("#224466").unwrap()));
    assert_eq!(calls(&app), quiet + 1, "one host call for the change");
    updates(&mut app, 10);
    assert_eq!(calls(&app), quiet + 1, "a has pair whose value holds makes no call");
    release(&mut app, go);
    app.update();
    assert_eq!(background(&app, go), Color::from(Srgba::hex("#3366cc").unwrap()));
    assert_eq!(calls(&app), quiet + 2, "absence is a value: the pair is kept and delivers false");
    press(&mut app, go);
    app.update();
    assert_eq!(background(&app, go), Color::from(Srgba::hex("#224466").unwrap()), "still watched after an absence");
}

#[test]
fn activate_runs_on_click_and_on_enter_and_not_while_disabled() {
    let _hang = common::watchdog("activate_runs_on_click_and_on_enter_and_not_while_disabled", BOUND);
    let (_dir, mut app) = setup("ui-events-activate", BUTTONS);
    let (go, early) = (node(&mut app, &["root", "go"]), node(&mut app, &["root", "early"]));
    press(&mut app, go);
    click(&mut app, go);
    release(&mut app, go);
    app.update();
    assert_eq!(text(&mut app, &["root", "log"]), "go ", "a click activates");
    key(&mut app, go, KeyCode::Enter, Key::Enter);
    app.update();
    assert_eq!(text(&mut app, &["root", "log"]), "go go ", "Enter on the focused button activates");
    press(&mut app, early);
    app.update();
    assert!(app.world().get::<ActivateOnPress>(early).is_some());
    assert_eq!(text(&mut app, &["root", "log"]), "go go early ", "activateOnPress activates on press");

    // Disable through the binding, then click and press Space: nothing.
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), "game::Off").unwrap();
    let flag = app.world_mut().spawn_empty().id();
    // SAFETY: Off is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(flag), id, &[1]) };
    updates(&mut app, 3);
    assert!(app.world().get::<InteractionDisabled>(go).is_some(), "disabled inserts InteractionDisabled");
    press(&mut app, go);
    click(&mut app, go);
    release(&mut app, go);
    key(&mut app, go, KeyCode::Space, Key::Space);
    updates(&mut app, 2);
    assert_eq!(text(&mut app, &["root", "log"]), "go go early ", "a disabled button does not activate");
}
