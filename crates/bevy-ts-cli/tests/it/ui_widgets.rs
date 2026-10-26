//! Stage 5 of bevy-ui-coverage: Bevy's value widgets as intrinsics (ui-widgets). Components and
//! load errors run headless. Pointer runs through the offscreen image and its camera, as
//! `ui_placement.rs` does, so picking is Bevy's own (macOS, Metal); the wheel, which the offscreen
//! pointer does not carry, is a triggered `PointerScroll`, as Bevy's widget tests trigger theirs.
//! Keys need a primary window, so the keyboard tests run in viewport mode, as `ui_focus.rs`'s do.

use crate::common;

use bevy::prelude::*;
use bevy::ui::{Checked, ScrollPosition, Selected};
use bevy::ui_widgets::{Scrollbar, ScrollbarThumb, SelectedTab, Slider, SliderRange, SliderStep, SliderThumb, SliderValue};
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts::log::LogLevel;
use bevy_ts_cli::cli::{Display, Engine, base_app};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::project::Switches;
use bevy_ts_tsx::TsxNode;
use common::{logs, messages, project, reload, write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) fn engine() -> Engine {
    Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") }
}

pub(crate) fn unfinished(display: Display, switches: Switches, entry: &Path) -> App {
    let mut app = base_app(&engine(), bevy_ts_cli::Prebuilt, display, Default::default(), None, true, switches, &[], false);
    app.insert_resource(DevSource(Arc::new(Compiler::new(entry, true).unwrap())));
    app
}

/// `game` loaded headless, a few frames run.
pub(crate) fn headless(name: &str, game: &str) -> (PathBuf, App) {
    let dir = project(name, &[("game.tsx", game)]);
    let mut app = unfinished(Display::Headless, Switches::default(), &dir.join("game.tsx"));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    common::updates(&mut app, 3);
    (dir, app)
}

/// The error a load of `game` fails with.
fn load_error(name: &str, game: &str) -> String {
    let dir = project(name, &[("game.tsx", game)]);
    let mut app = unfinished(Display::Headless, Switches::default(), &dir.join("game.tsx"));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    common::updates(&mut app, 2);
    let mut all: Vec<String> = r.errors.iter().map(|e| format!("{e:?}")).collect();
    all.extend(logs(&app).into_iter().filter(|e| e.level == LogLevel::Error).map(|e| e.message));
    assert!(!all.is_empty(), "{name}: loaded without an error");
    all.join("\n")
}

pub(crate) fn node(app: &mut App, path: &[&str]) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == path).map(|(e, _)| e).unwrap_or_else(|| panic!("no node {path:?}"))
}

pub(crate) fn said(app: &App, prefix: &str) -> Vec<String> {
    messages(app).into_iter().filter(|m| m.starts_with(prefix)).collect()
}

#[test]
fn a_slider_and_its_thumb_have_bevys_components() {
    let _hang = common::watchdog("a_slider_and_its_thumb_have_bevys_components", common::BOUND);
    let game = r#"import { app } from "bevy-ts";
app.scene("ui", () => (
  <slider key="s" initialValue={0.5} min={0} max={1} step={0.1}><sliderThumb key="t" /></slider>
));
"#;
    let (_dir, mut app) = headless("ui-widgets-slider", game);
    let s = node(&mut app, &["s"]);
    let t = node(&mut app, &["s", "t"]);
    let w = app.world();
    assert!(w.get::<Slider>(s).is_some());
    assert_eq!(w.get::<SliderValue>(s), Some(&SliderValue(0.5)));
    assert_eq!(w.get::<SliderRange>(s), Some(&SliderRange::new(0.0, 1.0)));
    assert_eq!(w.get::<SliderStep>(s), Some(&SliderStep(0.1)));
    assert!(w.get::<SliderThumb>(t).is_some());
}

#[test]
fn a_scrollbar_targets_its_ref_and_its_thumb_has_no_node() {
    let _hang = common::watchdog("a_scrollbar_targets_its_ref_and_its_thumb_has_no_node", common::BOUND);
    let game = r##"import { app, ref } from "bevy-ts";
const list = ref();
app.scene("ui", () => (
  <>
    <node key="list" ref={list} scrollArea style={{ overflow: { y: "scroll" } }} />
    <scrollbar key="bar" target={list} orientation="vertical"><scrollbarThumb key="thumb" background="#888" radius={4} style={{ border: 1 }} /></scrollbar>
  </>
));
"##;
    let (_dir, mut app) = headless("ui-widgets-scrollbar", game);
    let list = node(&mut app, &["list"]);
    let bar = node(&mut app, &["bar"]);
    let thumb = node(&mut app, &["bar", "thumb"]);
    let w = app.world();
    assert_eq!(w.get::<Scrollbar>(bar).map(|s| s.target), Some(list));
    assert!(w.get::<ScrollPosition>(list).is_some(), "scrollArea brings ScrollPosition");
    let t = w.get::<ScrollbarThumb>(thumb).expect("ScrollbarThumb");
    assert_eq!(t.border, UiRect::all(Val::Px(1.0)));
    assert!(w.get::<Node>(thumb).is_none(), "the scrollbar places the thumb: no Node");
}

#[test]
fn a_tab_list_selects_by_key() {
    let _hang = common::watchdog("a_tab_list_selects_by_key", common::BOUND);
    let game = |sel: &str| {
        format!(
            r#"import {{ app }} from "bevy-ts";
app.scene("ui", () => (
  <tabList key="tabs" selected={{{sel}}}><tab key="one" /><tab key="two" /></tabList>
));
"#
        )
    };
    let (dir, mut app) = headless("ui-widgets-tabs", &game("\"two\""));
    let tabs = node(&mut app, &["tabs"]);
    let two = node(&mut app, &["tabs", "two"]);
    assert_eq!(app.world().get::<SelectedTab>(tabs), Some(&SelectedTab(Some(two))));
    write(&dir, "game.tsx", &game("null"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    common::updates(&mut app, 2);
    assert_eq!(app.world().get::<SelectedTab>(tabs), Some(&SelectedTab(None)));
}

#[test]
fn a_tab_key_no_tab_has_warns_once() {
    let _hang = common::watchdog("a_tab_key_no_tab_has_warns_once", common::BOUND);
    let game = r#"import { app } from "bevy-ts";
app.scene("ui", () => <tabList key="tabs" selected="thre"><tab key="one" /></tabList>);
"#;
    let (_dir, mut app) = headless("ui-widgets-tab-typo", game);
    common::updates(&mut app, 3);
    let warned: Vec<_> = logs(&app).into_iter().filter(|l| l.level == LogLevel::Warn && l.message.contains("selects \"thre\"")).collect();
    assert_eq!(warned.len(), 1, "{warned:?}");
    let tabs = node(&mut app, &["tabs"]);
    assert_eq!(app.world().get::<SelectedTab>(tabs), Some(&SelectedTab(None)));
}

#[test]
fn both_forms_of_a_state_prop_fail_naming_it() {
    let _hang = common::watchdog("both_forms_of_a_state_prop_fail_naming_it", common::BOUND);
    let both = r#"import { app, signal } from "bevy-ts";
const [v] = signal(0);
// @ts-expect-error both forms
app.scene("ui", () => <slider key="s" value={() => v()} initialValue={0} />);
"#;
    let e = load_error("ui-widgets-both", both);
    assert!(e.contains("value or initialValue, not both"), "{e}");
}

#[test]
fn a_radio_group_mixing_forms_fails_naming_the_group() {
    let _hang = common::watchdog("a_radio_group_mixing_forms_fails_naming_the_group", common::BOUND);
    let mixed = r#"import { app } from "bevy-ts";
app.scene("ui", () => (
  <radioGroup key="g"><radio key="a" checked={true} /><radio key="b" initialChecked={false} /></radioGroup>
));
"#;
    let e = load_error("ui-widgets-mixed", mixed);
    assert!(e.contains("<radioGroup g>, whose radios mix checked (<a>) and initialChecked (<b>)"), "{e}");
}

/// A `<For>` hides its radios from the description: the mix is found among the mounted radios.
#[test]
fn a_radio_group_mixing_forms_through_a_for_fails_naming_the_group() {
    let _hang = common::watchdog("a_radio_group_mixing_forms_through_a_for_fails_naming_the_group", common::BOUND);
    let mixed = r#"import { app, For } from "bevy-ts";
app.scene("ui", () => (
  <radioGroup key="g">
    <For each={["a", "b"]} key={(k: string) => k}>
      {(k: string) => (k === "a" ? <radio key={k} checked={true} /> : <radio key={k} initialChecked={false} />)}
    </For>
  </radioGroup>
));
"#;
    let e = load_error("ui-widgets-mixed-for", mixed);
    assert!(e.contains("<radioGroup g>, whose radios mix checked (<a>) and initialChecked (<b>)"), "{e}");
}

#[test]
fn a_thumb_takes_only_its_border_from_style() {
    let _hang = common::watchdog("a_thumb_takes_only_its_border_from_style", common::BOUND);
    let wide = r#"import { app, ref } from "bevy-ts";
const l = ref();
app.scene("ui", () => (
  // @ts-expect-error a thumb has no width
  <scrollbar key="bar" target={l}><scrollbarThumb key="t" style={{ width: 4 }} /></scrollbar>
));
"#;
    let e = load_error("ui-widgets-thumb", wide);
    assert!(e.contains("style.width: a scrollbar thumb takes only style.border"), "{e}");
}

/// Pointer through the offscreen image and its camera.
#[cfg(target_os = "macos")]
mod pointer {
    use super::*;
    use bevy::camera::NormalizedRenderTarget;
    use bevy::input::mouse::MouseScrollUnit;
    use bevy::input::touch::TouchPhase;
    use bevy::picking::backend::HitData;
    use bevy::picking::events::{Pointer, PointerScroll};
    use bevy::picking::pointer::{Location, PointerId};
    use bevy_ts_remote::protocol::PointerEventKind::{self, Down, Move, Up};
    use bevy_ts_remote::protocol::{PointerEvent, PointerEventButton, PointerEventType};
    use bevy_ts_viewport::offscreen::{OffscreenInputPlugin, Pending};

    /// Every widget at a known place: rows 40 px apart, each part 40 by 20.
    const GAME: &str = r##"import { app, For, px, ref, signal } from "bevy-ts";
const at = (left: number, top: number, w = 40, h = 20) => ({ position: "absolute" as const, left: px(left), top: px(top), width: px(w), height: px(h) });
const cell = { width: px(40), height: px(20) };
const [sound, setSound] = signal(false);
const [tab, setTab] = signal<string | null>("one");
const [row, setRow] = signal<string | null>(null);
const [pick] = signal("x");
const list = ref();
app.scene("ui", () => (
  <>
    <checkbox key="controlled" style={at(0, 0)} checked={() => sound()} onValueChange={({ value }) => { console.log(`controlled ${value}`); setSound(value); }} />
    <checkbox key="bare" style={at(60, 0)} checked={() => false} />
    <checkbox key="initial" style={at(120, 0)} initialChecked={false} onValueChange={({ value }) => console.log(`initial ${value}`)} />
    <radioGroup key="level" style={at(0, 40, 200)} onValueChange={({ key, isFinal }) => console.log(`level ${key} ${isFinal}`)}>
      <radio key="easy" style={cell} />
      <radio key="hard" style={cell} />
      <checkbox key="extra" style={cell} />
    </radioGroup>
    <slider key="vol" style={at(0, 80, 200)} initialValue={0} min={0} max={100} onValueChange={({ value, isFinal }) => console.log(`vol ${Math.round(value)} ${isFinal}`)}>
      <sliderThumb key="thumb" style={{ width: px(10), height: px(20) }} />
    </slider>
    <tabList key="tabs" style={at(0, 120, 200)} selected={() => tab()} onValueChange={({ key }) => { console.log(`tab ${key}`); setTab(key); }}>
      <tab key="one" style={cell} />
      <tab key="two" style={cell} />
    </tabList>
    <listBox key="rows" style={{ ...at(0, 160, 40, 40), flexDirection: "column" }} onValueChange={({ key }) => { console.log(`row ${key}`); setRow(key); }}>
      <listItem key="r1" style={cell} selected={() => row() === "r1"} />
      <listItem key="r2" style={cell} selected={() => row() === "r2"} />
    </listBox>
    <radioGroup key="picked" style={at(0, 220, 200)}>
      <For each={["x", "y"]} key={(k: string) => k}>
        {(k: string) => <radio key={k} style={cell} checked={() => pick() === k} />}
      </For>
    </radioGroup>
    <node key="list" ref={list} scrollArea style={{ ...at(300, 0, 100, 100), flexDirection: "column", overflow: { y: "scroll" } }}>
      {Array.from({ length: 10 }, (_, i) => <node key={`i${i}`} style={{ height: px(40), flexShrink: 0 }} background="#888888" />)}
    </node>
    <scrollbar key="bar" target={list} orientation="vertical" style={at(400, 0, 10, 100)}>
      <scrollbarThumb key="thumb" background="#cccccc" />
    </scrollbar>
  </>
));
"##;

    fn scene(name: &str) -> (PathBuf, App) {
        let dir = project(name, &[("game.tsx", GAME)]);
        let mut app = unfinished(Display::Headless, Switches::default(), &dir.join("game.tsx"));
        let image = app.world().resource::<bevy_ts_cli::headless::Offscreen>().0.clone();
        app.add_plugins(OffscreenInputPlugin { image });
        app.finish();
        app.cleanup();
        app.world_mut().spawn(Camera2d);
        let r = reload(&mut app, false);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
        common::updates(&mut app, 5);
        (dir, app)
    }

    fn pointer(app: &mut App, kind: PointerEventKind, x: f64, y: f64) {
        app.world_mut().resource_mut::<Pending>().0.push_back(PointerEvent {
            button: Some(PointerEventButton::Left),
            buttons: vec![],
            clicks: None,
            dx: None,
            dy: None,
            kind,
            mods: None,
            type_: PointerEventType::Pointer,
            x,
            y,
        }.into());
        common::updates(app, 2);
    }

    fn click(app: &mut App, x: f64, y: f64) {
        for kind in [Move, Down, Up] {
            pointer(app, kind, x, y);
        }
        common::updates(app, 3);
    }

    /// Press at `from`, move to `to` in four steps, release.
    fn drag(app: &mut App, from: (f64, f64), to: (f64, f64)) {
        pointer(app, Move, from.0, from.1);
        pointer(app, Down, from.0, from.1);
        for i in 1..=4 {
            let t = i as f64 / 4.0;
            pointer(app, Move, from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        }
        pointer(app, Up, to.0, to.1);
        common::updates(app, 3);
    }

    fn checked(app: &mut App, path: &[&str]) -> bool {
        let e = node(app, path);
        app.world().get::<Checked>(e).is_some()
    }

    #[test]
    fn a_controlled_checkbox_follows_its_handler_and_one_without_never_changes() {
        let _hang = common::watchdog("a_controlled_checkbox_follows_its_handler_and_one_without_never_changes", common::BOUND);
        let (_dir, mut app) = scene("ui-widgets-check");
        click(&mut app, 20.0, 10.0);
        assert_eq!(said(&app, "controlled"), ["controlled true"]);
        assert!(checked(&mut app, &["controlled"]), "the handler set the signal and the binding inserted Checked");
        click(&mut app, 80.0, 10.0);
        assert!(!checked(&mut app, &["bare"]), "controlled, no handler: never Checked");
    }

    #[test]
    fn an_initial_checkbox_keeps_its_own_state_across_a_reload() {
        let _hang = common::watchdog("an_initial_checkbox_keeps_its_own_state_across_a_reload", common::BOUND);
        let (dir, mut app) = scene("ui-widgets-initial");
        click(&mut app, 140.0, 10.0);
        assert_eq!(said(&app, "initial"), ["initial true"]);
        assert!(checked(&mut app, &["initial"]));
        write(&dir, "game.tsx", &format!("{GAME}\n// edited\n"));
        assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
        common::updates(&mut app, 3);
        assert!(checked(&mut app, &["initial"]), "initialChecked is written at mount only");
    }

    #[test]
    fn a_radio_group_gives_the_key_and_not_a_child_checkboxs_change() {
        let _hang = common::watchdog("a_radio_group_gives_the_key_and_not_a_child_checkboxs_change", common::BOUND);
        let (_dir, mut app) = scene("ui-widgets-radio");
        click(&mut app, 60.0, 50.0);
        assert_eq!(said(&app, "level"), ["level hard true"]);
        assert!(checked(&mut app, &["level", "hard"]) && !checked(&mut app, &["level", "easy"]), "no radio has checked: the group updates itself");
        click(&mut app, 100.0, 50.0);
        assert!(checked(&mut app, &["level", "extra"]));
        assert_eq!(said(&app, "level"), ["level hard true"], "the group does not hear its checkbox");
    }

    /// Radios from a `<For>`, controlled with no handler: the group keeps no self-update.
    #[test]
    fn controlled_radios_from_a_for_never_change_without_a_handler() {
        let _hang = common::watchdog("controlled_radios_from_a_for_never_change_without_a_handler", common::BOUND);
        let (_dir, mut app) = scene("ui-widgets-radio-for");
        let radio = |app: &mut App, k: &str| {
            let w = app.world_mut();
            let e = w.query::<(Entity, &TsxNode)>().iter(w).find(|(_, n)| n.key_path.first().map(String::as_str) == Some("picked") && n.key_path.last().map(String::as_str) == Some(k));
            let e = e.map(|(e, _)| e).unwrap_or_else(|| panic!("no radio {k}"));
            w.get::<Checked>(e).is_some()
        };
        assert!(radio(&mut app, "x") && !radio(&mut app, "y"));
        click(&mut app, 60.0, 230.0);
        assert!(radio(&mut app, "x") && !radio(&mut app, "y"), "Bevy moved Checked on a controlled radio");
    }

    #[test]
    fn a_dragged_slider_is_not_final_until_released() {
        let _hang = common::watchdog("a_dragged_slider_is_not_final_until_released", common::BOUND);
        let (_dir, mut app) = scene("ui-widgets-drag");
        drag(&mut app, (5.0, 90.0), (105.0, 90.0));
        let log = said(&app, "vol");
        let (last, during) = log.split_last().expect("no slider change");
        assert!(!during.is_empty() && during.iter().all(|m| m.ends_with(" false")), "{log:?}");
        assert!(last.ends_with(" true"), "{log:?}");
        let s = node(&mut app, &["vol"]);
        let v = app.world().get::<SliderValue>(s).unwrap().0;
        assert!(v > 30.0 && v < 70.0, "initialValue: the slider moved itself to {v}");
    }

    #[test]
    fn a_scrollbar_drag_writes_its_targets_scroll_position() {
        let _hang = common::watchdog("a_scrollbar_drag_writes_its_targets_scroll_position", common::BOUND);
        let (_dir, mut app) = scene("ui-widgets-bar");
        let list = node(&mut app, &["list"]);
        assert_eq!(app.world().get::<ScrollPosition>(list).unwrap().y, 0.0);
        drag(&mut app, (405.0, 5.0), (405.0, 45.0));
        let y = app.world().get::<ScrollPosition>(list).unwrap().y;
        assert!(y > 100.0, "the thumb moved 40 of 100 px over 400 px of content: scrolled {y}");
    }

    #[test]
    fn a_scroll_area_scrolls_by_the_wheel() {
        let _hang = common::watchdog("a_scroll_area_scrolls_by_the_wheel", common::BOUND);
        let (_dir, mut app) = scene("ui-widgets-wheel");
        let list = node(&mut app, &["list"]);
        let camera = app.world_mut().query_filtered::<Entity, With<Camera2d>>().single(app.world()).unwrap();
        app.world_mut().trigger(PointerScroll {
            entity: list,
            pointer: Pointer::new(PointerId::Mouse, Location { target: NormalizedRenderTarget::None { width: 960, height: 540 }, position: Vec2::new(350.0, 50.0) }),
            unit: MouseScrollUnit::Pixel,
            x: 0.0,
            y: -60.0,
            hit: HitData::new(camera, 0.0, None, None),
            phase: TouchPhase::Moved,
        });
        common::updates(&mut app, 2);
        assert_eq!(app.world().get::<ScrollPosition>(list).unwrap().y, 60.0);
    }

    #[test]
    fn a_tab_list_and_a_list_box_give_keys_and_follow_the_game() {
        let _hang = common::watchdog("a_tab_list_and_a_list_box_give_keys_and_follow_the_game", common::BOUND);
        let (_dir, mut app) = scene("ui-widgets-tab-list");
        let tabs = node(&mut app, &["tabs"]);
        let one = node(&mut app, &["tabs", "one"]);
        let two = node(&mut app, &["tabs", "two"]);
        assert_eq!(app.world().get::<SelectedTab>(tabs), Some(&SelectedTab(Some(one))));
        click(&mut app, 60.0, 130.0);
        assert_eq!(said(&app, "tab"), ["tab two"]);
        assert_eq!(app.world().get::<SelectedTab>(tabs), Some(&SelectedTab(Some(two))), "the handler set the key");

        click(&mut app, 20.0, 190.0);
        assert_eq!(said(&app, "row"), ["row r2"]);
        let (r1, r2) = (node(&mut app, &["rows", "r1"]), node(&mut app, &["rows", "r2"]));
        assert!(app.world().get::<Selected>(r2).is_some() && app.world().get::<Selected>(r1).is_none());
    }
}

/// Keys in viewport mode, as `bevy-ts dev --viewport` builds the app.
#[cfg(target_os = "macos")]
pub(crate) mod keys {
    use super::*;
    use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
    use serde_json::{Value, json};
    use std::time::Duration;

    const TAB: Switches = Switches { mesh_picking: false, tab_navigation: true, directional_navigation: false };

    pub(crate) fn call(app: &mut App, method: &str, params: Value) -> Value {
        let id = match app.world().resource::<RemoteMethods>().get(method) {
            Some(RemoteMethodSystemId::Instant(id)) => *id,
            _ => panic!("{method} not served"),
        };
        app.world_mut().run_system_with(id, (!params.is_null()).then_some(params)).unwrap().unwrap_or_else(|e| panic!("{method}: {}", e.message))
    }

    pub(crate) fn updates(app: &mut App, n: usize) {
        for _ in 0..n {
            std::thread::sleep(Duration::from_millis(5));
            app.update();
        }
    }

    pub(crate) fn start(name: &str, game: &str) -> App {
        let dir = project(name, &[("game.tsx", game)]);
        let mut app = unfinished(Display::Viewport, TAB, &dir.join("game.tsx"));
        app.add_plugins(RemotePlugin::default());
        bevy_ts_remote::add_methods(&mut app);
        app.finish();
        app.cleanup();
        app.update();
        let r = reload(&mut app, false);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
        call(&mut app, "bevy-ts/viewport_attach", json!({ "w": 400, "h": 300, "scale": 1 }));
        updates(&mut app, 4);
        app
    }

    pub(crate) fn press(app: &mut App, code: &str) {
        let key = if code == "Space" { " " } else { code };
        call(app, "bevy-ts/viewport_input", json!({ "events": [{ "type": "key", "state": "down", "code": code, "key": key }] }));
        updates(app, 2);
        call(app, "bevy-ts/viewport_input", json!({ "events": [{ "type": "key", "state": "up", "code": code, "key": key }] }));
        updates(app, 2);
    }

    /// Each widget focused at mount, its keys pressed, and what its `onValueChange` said.
    #[test]
    fn each_widget_answers_its_keys() {
        let _hang = common::watchdog("each_widget_answers_its_keys", common::BOUND);
        let cases: [(&str, &str, &str); 5] = [
            ("check", r#"<checkbox key="w" tabIndex={0} autoFocus onValueChange={({ value }) => console.log(`said ${value}`)} />"#, "Space"),
            ("slider", r#"<slider key="w" tabIndex={0} autoFocus initialValue={2} min={0} max={10} onValueChange={({ value }) => console.log(`said ${value}`)} />"#, "ArrowRight"),
            ("radio", r#"<radioGroup key="w" tabIndex={0} autoFocus onValueChange={({ key }) => console.log(`said ${key}`)}><radio key="a" /><radio key="b" /></radioGroup>"#, "ArrowDown"),
            ("tabs", r#"<tabList key="w" initialSelected="a" activation="automatic" onValueChange={({ key }) => console.log(`said ${key}`)}><tab key="a" tabIndex={0} autoFocus /><tab key="b" tabIndex={0} /></tabList>"#, "ArrowRight"),
            ("list", r#"<listBox key="w" tabIndex={0} autoFocus onValueChange={({ key }) => console.log(`said ${key}`)}><listItem key="a" /><listItem key="b" /></listBox>"#, "ArrowDown Space"),
        ];
        let expected = ["said true", "said 3", "said a", "said b", "said b"];
        for ((name, element, key), want) in cases.into_iter().zip(expected) {
            let game = format!("import {{ app }} from \"bevy-ts\";\napp.scene(\"ui\", () => <node key=\"g\" tabGroup={{{{}}}}>{element}</node>);\n");
            let mut app = start(&format!("ui-widgets-keys-{name}"), &game);
            key.split(' ').for_each(|k| press(&mut app, k));
            assert_eq!(said(&app, "said"), [want], "{name}: {key}");
        }
    }
}
