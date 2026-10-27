//! Stage 4 of bevy-ui-coverage: focus and navigation. `tabNavigation` and `directionalNavigation`
//! widen mesh-picking's switch; `tabIndex`, `tabGroup` and `autoFocus` on every UI element;
//! `onFocusGained` and `onFocusLost`; `InputFocusVisible` and `DirectionalNavigation` in
//! `bevy-ts/input`. Keys need a primary window, so the keyboard tests run in viewport mode (macOS),
//! as text-input's do; the rest run headless.

use crate::common;

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::directional_navigation::{DirectionalNavigationMap, DirectionalNavigationPlugin};
use bevy::input_focus::tab_navigation::{TabIndex, TabNavigationPlugin};
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts::log::LogLevel;
use bevy_ts_cli::cli::{Display, Engine, base_app};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::project::Switches;
use bevy_ts_tsx::TsxNode;
use common::{logs, messages, project, reload, write};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

const BOTH: Switches = Switches { mesh_picking: false, tab_navigation: true, directional_navigation: true, image_sampler: bevy_ts_cli::project::DefaultSampler::Linear };

fn engine() -> Engine {
    Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") }
}

/// The engine `bevy-ts dev` builds, before `finish`, with `entry` as its source.
fn unfinished(extra: impl Plugin, display: Display, switches: Switches, entry: &Path) -> App {
    let mut app = base_app(&engine(), extra, display, Default::default(), None, true, switches, &[], false);
    app.insert_resource(DevSource(Arc::new(Compiler::new(entry, true).unwrap())));
    app
}

/// `game` loaded headless with `switches`, a few frames run.
fn headless(name: &str, game: &str, extra: impl Plugin, switches: Switches) -> (crate::common::TestDir, App) {
    let dir = project(name, &[("game.tsx", game)]);
    let mut app = unfinished(extra, Display::Headless, switches, &dir.join("game.tsx"));
    app.finish();
    app.cleanup();
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    common::updates(&mut app, 3);
    app.insert_resource(dir.clone());
    (dir, app)
}

fn node(app: &mut App, path: &[&str]) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == path).map(|(e, _)| e).unwrap_or_else(|| panic!("no node {path:?}"))
}

/// The tsx key of the focused entity, if one has focus.
fn focused(app: &mut App) -> Option<String> {
    let e = app.world().resource::<InputFocus>().get()?;
    app.world().get::<TsxNode>(e).map(|n| n.key_path.last().cloned().unwrap_or_default())
}

fn logged(app: &App, prefix: &str) -> Vec<String> {
    messages(app).into_iter().filter(|m| m.starts_with(prefix)).collect()
}

fn tab_warnings(app: &App) -> Vec<String> {
    logs(app).into_iter().filter(|e| e.level == LogLevel::Warn && e.message.contains("tabNavigation")).map(|e| e.message).collect()
}

const TWO_TABS: &str = r#"import { app } from "bevy-ts";
app.scene("ui", () => (
  <node key="g" tabGroup={{}}>
    <button key="first" tabIndex={0} />
    <button key="second" tabIndex={0} />
    <input key="field" />
  </node>
));
"#;

#[test]
fn a_tab_index_with_tab_navigation_off_is_named_once() {
    let (_dir, mut app) = headless("ui-focus-warn", TWO_TABS, bevy_ts_cli::Prebuilt, Switches::default());
    let w = tab_warnings(&app);
    assert_eq!(w.len(), 1, "{w:#?}");
    assert!(w[0].contains("\"tabNavigation\": true") && w[0].contains("bevy-ts.json"), "{}", w[0]);
    // The props still write Bevy's components: inert without the plugin, as in Bevy.
    let first = node(&mut app, &["g", "first"]);
    assert_eq!(app.world().get::<TabIndex>(first), Some(&TabIndex(0)));
    assert!(!app.is_plugin_added::<TabNavigationPlugin>());
    assert!(!app.world().contains_resource::<DirectionalNavigationMap>());
}

#[test]
fn no_tab_warning_with_the_switch() {
    let (_dir, app) = headless("ui-focus-warn-on", TWO_TABS, bevy_ts_cli::Prebuilt, BOTH);
    assert_eq!(tab_warnings(&app), Vec::<String>::new());
}

/// A native crate's plugin that adds tab navigation itself.
struct Native;

impl Plugin for Native {
    fn build(&self, app: &mut App) {
        app.add_plugins(TabNavigationPlugin);
    }
}

#[test]
fn no_tab_warning_when_a_native_plugin_added_it() {
    let (_dir, app) = headless("ui-focus-warn-native", TWO_TABS, Native, Switches::default());
    assert_eq!(tab_warnings(&app), Vec::<String>::new());
    // With the switch as well, it is added once: a second add would panic.
    let (_dir, app) = headless("ui-focus-warn-native-on", TWO_TABS, Native, BOTH);
    assert!(app.is_plugin_added::<TabNavigationPlugin>());
}

fn field(tab_index: &str) -> String {
    format!(
        r#"import {{ app }} from "bevy-ts";
app.scene("ui", () => <input key="field"{tab_index} />);
"#
    )
}

#[test]
fn an_inputs_own_tab_index_is_not_a_prop() {
    let (dir, mut app) = headless("ui-focus-input", &field(""), bevy_ts_cli::Prebuilt, Switches::default());
    let f = node(&mut app, &["field"]);
    assert_eq!(app.world().get::<TabIndex>(f), Some(&TabIndex(0)), "an <input> is in the tab order by default");
    assert_eq!(tab_warnings(&app), Vec::<String>::new(), "its default TabIndex is no prop");
    // A tabIndex prop replaces it; removing the prop goes back to TabIndex(0), not to none.
    write(&dir, "game.tsx", &field(" tabIndex={3}"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    common::updates(&mut app, 2);
    assert_eq!(app.world().get::<TabIndex>(f), Some(&TabIndex(3)));
    write(&dir, "game.tsx", &field(""));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    common::updates(&mut app, 2);
    assert_eq!(app.world().get::<TabIndex>(f), Some(&TabIndex(0)));
}

#[test]
fn removing_tab_index_from_a_button_removes_it() {
    let game = |props: &str| format!("import {{ app }} from \"bevy-ts\";\napp.scene(\"ui\", () => <button key=\"b\"{props} />);\n");
    let (dir, mut app) = headless("ui-focus-remove", &game(" tabIndex={1} tabGroup={{ order: 2, modal: true }} autoFocus"), bevy_ts_cli::Prebuilt, BOTH);
    let b = node(&mut app, &["b"]);
    assert_eq!(app.world().get::<TabIndex>(b), Some(&TabIndex(1)));
    let group = app.world().get::<bevy::input_focus::tab_navigation::TabGroup>(b).unwrap();
    assert_eq!((group.order, group.modal), (2, true));
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(b), "autoFocus on a button");
    write(&dir, "game.tsx", &game(""));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    common::updates(&mut app, 2);
    assert!(app.world().get::<TabIndex>(b).is_none());
    assert!(app.world().get::<bevy::input_focus::tab_navigation::TabGroup>(b).is_none());
}

#[test]
fn a_bad_tab_index_or_group_is_a_load_error() {
    for (name, props, needle) in [("ui-focus-bad-index", "tabIndex={1.5}", "tabIndex"), ("ui-focus-bad-group", "tabGroup={{ modal: 1 }}", "tabGroup")] {
        let dir = project(name, &[("game.tsx", &format!("import {{ app }} from \"bevy-ts\";\napp.scene(\"ui\", () => <button key=\"b\" {props} />);\n"))]);
        let mut app = unfinished(bevy_ts_cli::Prebuilt, Display::Headless, BOTH, &dir.join("game.tsx"));
        app.finish();
        app.cleanup();
        let r = reload(&mut app, false);
        common::updates(&mut app, 2);
        let all = format!("{:?} {:?}", r.errors, messages(&app));
        assert!(all.contains(needle), "{needle} in {all}");
    }
}

#[test]
fn headless_both_switches_load_and_tab_does_nothing() {
    let (_dir, mut app) = headless("ui-focus-headless", TWO_TABS, bevy_ts_cli::Prebuilt, BOTH);
    assert!(app.is_plugin_added::<TabNavigationPlugin>() && app.is_plugin_added::<DirectionalNavigationPlugin>());
    assert!(app.world().contains_resource::<DirectionalNavigationMap>());
    // No primary window: the plugin observes nothing, and a Tab reaches nobody.
    let window = app.world_mut().spawn(Window::default()).id();
    app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::Tab,
        logical_key: Key::Tab,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window,
    });
    common::updates(&mut app, 2);
    assert_eq!(focused(&mut app), None);
}

const NAVIGATE: &str = r#"import { app, Update } from "bevy-ts";
import { CompassOctant, type DirectionalNavigation } from "bevy-ts/input";

let done = false;
export function go(nav: DirectionalNavigation) {
  if (done) return;
  done = true;
  try {
    console.log(`moved ${nav.navigate(CompassOctant.South)}`);
  } catch (e) {
    console.log(`threw ${(e as Error).message}`);
  }
}
app.addSystems(Update, go);
"#;

#[test]
fn navigate_throws_with_directional_navigation_off() {
    let (_dir, app) = headless("ui-focus-navigate-off", NAVIGATE, bevy_ts_cli::Prebuilt, Switches::default());
    let threw = logged(&app, "threw");
    assert_eq!(threw.len(), 1, "{:?}", messages(&app));
    assert!(threw[0].contains("\"directionalNavigation\": true") && threw[0].contains("bevy-ts.json"), "{}", threw[0]);
    // With the switch, no focus: no neighbour, so undefined.
    let (_dir, app) = headless("ui-focus-navigate-on", NAVIGATE, bevy_ts_cli::Prebuilt, BOTH);
    assert_eq!(logged(&app, "moved"), vec!["moved undefined"], "{:?}", messages(&app));
}

#[test]
fn a_wrong_navigation_switch_type_fails_dev() {
    for field in ["tabNavigation", "directionalNavigation"] {
        let config = format!(r#"{{ "{field}": "yes" }}"#);
        let dir = project(&format!("ui-focus-wrong-{field}"), &[("bevy-ts.json", &config), ("tsconfig.json", "{}"), ("src/game.tsx", TWO_TABS)]);
        let out = Command::new(common::BIN).args(["dev", dir.join("src/game.tsx").to_str().unwrap(), "--headless"]).output().unwrap();
        assert_eq!(out.status.code(), Some(1));
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains(field) && err.contains("bevy-ts.json"), "{err}");
    }
}

/// Keys in viewport mode, as `bevy-ts dev --viewport` builds the app.
#[cfg(target_os = "macos")]
mod viewport {
    use super::*;
    use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
    use serde_json::{Value, json};
    use std::time::Duration;

    fn call(app: &mut App, method: &str, params: Value) -> Value {
        let id = match app.world().resource::<RemoteMethods>().get(method) {
            Some(RemoteMethodSystemId::Instant(id)) => *id,
            _ => panic!("{method} not served"),
        };
        app.world_mut().run_system_with(id, (!params.is_null()).then_some(params)).unwrap().unwrap_or_else(|e| panic!("{method}: {}", e.message))
    }

    fn updates(app: &mut App, n: usize) {
        for _ in 0..n {
            std::thread::sleep(Duration::from_millis(5));
            app.update();
        }
    }

    /// `game.tsx` loaded in viewport mode with `switches`, a viewer attached; in edit mode when
    /// `edit`, as `bevy-ts dev --viewport --edit` boots.
    fn start(name: &str, game: &str, switches: Switches, edit: bool) -> (crate::common::TestDir, App) {
        let dir = project(name, &[("game.tsx", game)]);
        let mut app = unfinished(bevy_ts_cli::Prebuilt, Display::Viewport, switches, &dir.join("game.tsx"));
        app.add_plugins(RemotePlugin::default());
        bevy_ts_remote::add_methods(&mut app);
        if edit {
            app.insert_state(bevy_ts::EngineMode::Edit);
            app.world_mut().resource_mut::<bevy_ts::generation::Generations>().started = false;
            app.world_mut().resource_mut::<bevy_ts::runtime::ScriptClock>().paused = true;
        }
        app.finish();
        app.cleanup();
        app.update();
        let r = reload(&mut app, false);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
        call(&mut app, "bevy-ts/viewport_attach", json!({ "w": 400, "h": 300, "scale": 1 }));
        updates(&mut app, 4);
        app.insert_resource(dir.clone());
        (dir, app)
    }

    /// One key pressed and released, as a viewer sends it.
    fn press(app: &mut App, code: &str) {
        call(app, "bevy-ts/viewport_input", json!({ "events": [{ "type": "key", "state": "down", "code": code, "key": code }] }));
        updates(app, 2);
        call(app, "bevy-ts/viewport_input", json!({ "events": [{ "type": "key", "state": "up", "code": code, "key": code }] }));
        updates(app, 2);
    }

    const THREE: &str = r#"import { app, Update } from "bevy-ts";
import type { InputFocusVisible } from "bevy-ts/input";
app.scene("ui", () => (
  <node key="g" tabGroup={{}}>
    <button key="a" tabIndex={0} onFocusGained={(e) => console.log(`gained a ${e.cause}`)} onFocusLost={() => console.log("lost a")} />
    <button key="b" tabIndex={0} onFocusGained={(e) => console.log(`gained b ${e.cause}`)} onFocusLost={() => console.log("lost b")}>
      <node key="inner" tabIndex={0} />
    </button>
    <button key="c" tabIndex={0} onFocusGained={(e) => console.log(`gained c ${e.cause}`)} onFocusLost={() => console.log("lost c")} />
  </node>
));
export function visible(v: InputFocusVisible) { if (v.get()) console.log("visible"); }
app.addSystems(Update, visible);
"#;

    fn focus_log(app: &App) -> Vec<String> {
        messages(app).into_iter().filter(|m| m.starts_with("gained") || m.starts_with("lost")).collect()
    }

    #[test]
    fn tab_moves_through_three_buttons_with_focus_events() {
        let (_dir, mut app) = start("ui-focus-tab", THREE, BOTH, false);
        assert_eq!(focused(&mut app), None);
        press(&mut app, "Tab");
        assert_eq!(focused(&mut app).as_deref(), Some("a"));
        press(&mut app, "Tab");
        assert_eq!(focused(&mut app).as_deref(), Some("b"));
        // b's own child takes focus next: b hears neither, as its handlers are entity-local.
        press(&mut app, "Tab");
        assert_eq!(focused(&mut app).as_deref(), Some("inner"));
        press(&mut app, "Tab");
        assert_eq!(focused(&mut app).as_deref(), Some("c"));
        assert_eq!(focus_log(&app), ["gained a navigated", "lost a", "gained b navigated", "lost b", "gained c navigated"]);
        assert!(!logged(&app, "visible").is_empty(), "Tab makes focus visible");
    }

    #[test]
    fn a_modal_tab_group_traps_focus() {
        let game = r#"import { app } from "bevy-ts";
app.scene("ui", () => (
  <node key="g" tabGroup={{}}>
    <button key="out" tabIndex={0} />
    <node key="dialog" tabGroup={{ modal: true }}>
      <button key="m1" tabIndex={0} autoFocus />
      <button key="m2" tabIndex={0} />
    </node>
  </node>
));
"#;
        let (_dir, mut app) = start("ui-focus-modal", game, BOTH, false);
        assert_eq!(focused(&mut app).as_deref(), Some("m1"));
        let mut seen = vec![];
        for _ in 0..4 {
            press(&mut app, "Tab");
            seen.push(focused(&mut app).unwrap_or_default());
        }
        assert_eq!(seen, ["m2", "m1", "m2", "m1"]);
    }

    #[test]
    fn arrow_keys_move_focus_through_directional_navigation() {
        let game = r#"import { app, px, Update } from "bevy-ts";
import { AutoDirectionalNavigation, Camera2d } from "bevy-ts/bevy";
import { CompassOctant, KeyCode, type DirectionalNavigation, type Keys } from "bevy-ts/input";

export function arrows(keys: Keys, nav: DirectionalNavigation) {
  if (keys.justPressed(KeyCode.ArrowDown)) nav.navigate(CompassOctant.South);
  if (keys.justPressed(KeyCode.ArrowUp)) nav.navigate(CompassOctant.North);
}
app.addSystems(Update, arrows);
app.scene("ui", () => (
  <>
    <entity key="camera"><Camera2d /></entity>
    <node key="col" style={{ flexDirection: "column", rowGap: px(10) }}>
      <button key="top" autoFocus style={{ width: px(80), height: px(30) }}><AutoDirectionalNavigation /></button>
      <button key="middle" style={{ width: px(80), height: px(30) }}><AutoDirectionalNavigation /></button>
      <button key="bottom" style={{ width: px(80), height: px(30) }}><AutoDirectionalNavigation /></button>
    </node>
  </>
));
"#;
        let (_dir, mut app) = start("ui-focus-arrows", game, BOTH, false);
        assert_eq!(focused(&mut app).as_deref(), Some("top"));
        press(&mut app, "ArrowDown");
        assert_eq!(focused(&mut app).as_deref(), Some("middle"));
        press(&mut app, "ArrowDown");
        assert_eq!(focused(&mut app).as_deref(), Some("bottom"));
        press(&mut app, "ArrowUp");
        assert_eq!(focused(&mut app).as_deref(), Some("middle"));
    }

    #[test]
    fn off_by_default_tab_moves_nothing() {
        let (_dir, mut app) = start("ui-focus-off", THREE, Switches::default(), false);
        press(&mut app, "Tab");
        assert_eq!(focused(&mut app), None);
        assert!(!app.world().contains_resource::<DirectionalNavigationMap>());
    }

    #[test]
    fn tab_while_editing_does_not_move_game_focus() {
        let (_dir, mut app) = start("ui-focus-edit", THREE, BOTH, true);
        press(&mut app, "Tab");
        assert_eq!(focused(&mut app), None);
    }

    #[test]
    fn a_field_types_nothing_while_editing() {
        let game = r#"import { app } from "bevy-ts";
app.scene("ui", () => <input key="field" autoFocus onKeyboardInput={(e) => console.log(`key ${e.state} ${e.code}`)} />);
"#;
        let (_dir, mut app) = start("ui-focus-edit-field", game, BOTH, true);
        press(&mut app, "KeyA");
        assert_eq!(logged(&app, "key "), Vec::<String>::new());
        let f = node(&mut app, &["field"]);
        assert_eq!(app.world().get::<bevy::text::EditableText>(f).unwrap().value().to_string(), "");
        // In Play the same key types.
        call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "play" }));
        updates(&mut app, 4);
        press(&mut app, "KeyA");
        assert_eq!(logged(&app, "key "), ["key down KeyA", "key up KeyA"], "the edit-mode press does not arrive late");
    }

    #[test]
    fn play_stop_play_then_one_tab_moves_one_step() {
        let (_dir, mut app) = start("ui-focus-replay", THREE, BOTH, true);
        for mode in ["play", "edit", "play"] {
            call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": mode }));
            updates(&mut app, 6);
        }
        press(&mut app, "Tab");
        assert_eq!(focused(&mut app).as_deref(), Some("a"));
        press(&mut app, "Tab");
        assert_eq!(focused(&mut app).as_deref(), Some("b"));
    }
}
