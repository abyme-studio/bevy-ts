//! Stage 6 of bevy-ui-coverage: Bevy's overlays as intrinsics (ui-widgets): menus, popovers,
//! dialogs. Components run headless. Pointer and keys run in viewport mode, through
//! `bevy-ts/viewport_input`, as `ui_widgets.rs`'s keys do: a dialog's drag reads the primary window,
//! and a modal's Escape needs focus, which Bevy gives the dialog 0.1 s after it mounts.

use crate::common;
use crate::ui_widgets::{headless, node};
use bevy::input_focus::tab_navigation::{TabGroup, TabIndex};
use bevy::ui_widgets::popover::Popover;
use bevy::ui_widgets::{Dialog, DialogDragHandle, MenuButton, MenuFocusState, MenuItem, MenuPopup, ModalDialog, ModalDialogBarrier};

#[test]
fn overlays_have_bevys_components() {
    let _hang = common::watchdog("overlays_have_bevys_components", common::BOUND);
    let game = r#"import { app } from "bevy-ts";
app.scene("ui", () => (
  <>
    <menuButton key="button" />
    <menuPopup key="popup" layout="row"><menuItem key="item" /><menuItem key="later" tabIndex={2} /></menuPopup>
    <node key="anchor"><popover key="pop" positions={[{ side: "right", gap: 2 }]} windowMargin={6} /></node>
    <modalDialogBarrier key="barrier"><dialog key="d" modal><dialogDragHandle key="handle" /></dialog></modalDialogBarrier>
    <dialog key="free" />
  </>
));
"#;
    let (_dir, mut app) = headless("ui-overlays-components", game);
    let [button, popup, item, later, pop, barrier, d, handle, free] =
        [&["button"][..], &["popup"], &["popup", "item"], &["popup", "later"], &["anchor", "pop"], &["barrier"], &["barrier", "d"], &["barrier", "d", "handle"], &["free"]]
            .map(|p| node(&mut app, p));
    let w = app.world();
    assert!(w.get::<MenuButton>(button).is_some());
    assert!(w.get::<MenuPopup>(popup).is_some_and(|m| m.layout == bevy::ui_widgets::MenuLayout::Row));
    assert!(w.get::<MenuFocusState>(popup).is_some(), "a popup opens on mount");
    assert!(w.get::<MenuItem>(item).is_some());
    assert_eq!(w.get::<TabIndex>(item), Some(&TabIndex(0)), "an item is focusable with no tabIndex");
    assert_eq!(w.get::<TabIndex>(later), Some(&TabIndex(2)));
    let p = w.get::<Popover>(pop).expect("Popover");
    assert_eq!((p.positions.len(), p.positions[0].gap, p.window_margin), (1, 2.0, 6.0));
    assert!(w.get::<ModalDialogBarrier>(barrier).is_some());
    assert!(w.get::<ModalDialog>(d).is_some() && w.get::<TabGroup>(d).is_some_and(|g| g.modal));
    assert!(w.get::<DialogDragHandle>(handle).is_some());
    assert!(w.get::<Dialog>(free).is_some() && w.get::<ModalDialog>(free).is_none());
}

/// Pointer and keys in viewport mode.
#[cfg(target_os = "macos")]
mod viewport {
    use super::*;
    use crate::ui_widgets::keys::{call, press, start, updates};
    use crate::ui_widgets::said;
    use bevy::prelude::*;
    use bevy::ui::{UiGlobalTransform, UiTransform, Val2};
    use serde_json::{Value, json};

    fn pointer(kind: &str, x: f64, y: f64) -> Value {
        json!({ "type": "pointer", "kind": kind, "x": x, "y": y, "button": "left" })
    }

    /// One event a frame: viewport mode applies a call's events in one frame, and picking sends a
    /// release to what was under the pointer the frame before.
    fn send(app: &mut App, events: &[Value]) {
        for e in events {
            call(app, "bevy-ts/viewport_input", json!({ "events": [e] }));
            updates(app, 2);
        }
    }

    fn click(app: &mut App, x: f64, y: f64) {
        send(app, &[pointer("move", x, y), pointer("down", x, y), pointer("up", x, y)]);
        updates(app, 2);
    }

    /// A menu whose owner opens it on `open-*` and `toggle`, and closes it on `close-all`.
    const MENU: &str = r#"import { app, px, signal, Show } from "bevy-ts";
const [open, setOpen] = signal(false);
const cell = { width: px(100), height: px(20) };
app.scene("ui", () => (
  <node key="menu" style={{ position: "absolute", left: 0, top: 0, flexDirection: "column" }}
    onMenuEvent={({ action }) => {
      console.log(`menu ${action}`);
      if (action === "toggle") setOpen(!open());
      else if (action.startsWith("open")) setOpen(true);
      else if (action === "close-all") setOpen(false);
    }}>
    <menuButton key="button" tabIndex={0} autoFocus style={cell} />
    <Show when={open}>
      <menuPopup key="popup" style={{ flexDirection: "column" }}>
        <menuItem key="a" style={cell} onActivate={() => console.log("item a")} />
        <menuItem key="b" style={cell} onActivate={() => console.log("item b")} />
      </menuPopup>
    </Show>
  </node>
));
"#;

    fn popup_open(app: &mut App) -> bool {
        let w = app.world_mut();
        w.query_filtered::<(), With<MenuPopup>>().iter(w).next().is_some()
    }

    #[test]
    fn a_menu_opens_by_its_button_and_closes_after_an_item() {
        let _hang = common::watchdog("a_menu_opens_by_its_button_and_closes_after_an_item", common::BOUND);
        let mut app = start("ui-overlays-menu-pointer", MENU);
        click(&mut app, 50.0, 10.0);
        assert_eq!(said(&app, "menu"), ["menu toggle"]);
        assert!(popup_open(&mut app), "the owner opened the popup");
        click(&mut app, 50.0, 50.0);
        assert_eq!(said(&app, "item"), ["item b"]);
        assert_eq!(said(&app, "menu"), ["menu toggle", "menu focus-root", "menu close-all"]);
        assert!(!popup_open(&mut app));
    }

    #[test]
    fn a_menu_opens_navigates_and_activates_by_keys() {
        let _hang = common::watchdog("a_menu_opens_navigates_and_activates_by_keys", common::BOUND);
        let mut app = start("ui-overlays-menu-keys", MENU);
        press(&mut app, "ArrowDown");
        assert_eq!(said(&app, "menu"), ["menu open-first"]);
        updates(&mut app, 3);
        assert!(popup_open(&mut app));
        press(&mut app, "ArrowDown");
        press(&mut app, "Enter");
        assert_eq!(said(&app, "item"), ["item b"], "focus went to a on opening, then down to b");
        assert!(said(&app, "menu").contains(&"menu close-all".to_string()), "{:?}", said(&app, "menu"));
        assert!(!popup_open(&mut app));
    }

    #[test]
    fn a_popover_sits_against_its_parent() {
        let _hang = common::watchdog("a_popover_sits_against_its_parent", common::BOUND);
        let game = r#"import { app, px } from "bevy-ts";
app.scene("ui", () => (
  <node key="anchor" style={{ position: "absolute", left: px(100), top: px(100), width: px(40), height: px(20) }}>
    <popover key="pop" positions={[{ side: "bottom", align: "start", gap: 4 }]} style={{ width: px(30), height: px(10) }} />
  </node>
));
"#;
        let mut app = start("ui-overlays-popover", game);
        updates(&mut app, 3);
        let pop = node(&mut app, &["anchor", "pop"]);
        let at = app.world().get::<UiGlobalTransform>(pop).expect("placed").translation;
        // Below the anchor (100..120) by the gap, its left edge on the anchor's: centre (115, 129).
        assert!((at - Vec2::new(115.0, 129.0)).length() < 1.0, "popover centre {at}");
    }

    const MODAL: &str = r#"import { app, percent, px } from "bevy-ts";
app.scene("ui", () => (
  <modalDialogBarrier key="barrier" style={{ position: "absolute", width: percent(100), height: percent(100) }}
    onRequestClose={({ entity, source }) => console.log(`close ${source === entity}`)}>
    <dialog key="d" modal style={{ position: "absolute", left: px(150), top: px(100), width: px(100), height: px(60), flexDirection: "column" }}>
      <dialogDragHandle key="handle" style={{ width: px(100), height: px(20) }} />
      <button key="ok" tabIndex={0} autoFocus style={{ width: px(40), height: px(20) }} />
    </dialog>
  </modalDialogBarrier>
));
"#;

    #[test]
    fn a_modal_barrier_asks_to_close_on_a_click_and_on_escape() {
        let _hang = common::watchdog("a_modal_barrier_asks_to_close_on_a_click_and_on_escape", common::BOUND);
        let mut app = start("ui-overlays-modal", MODAL);
        // Escape reaches the barrier from the focused OK. Bevy would focus the dialog's last
        // focusable 0.1 s after it mounts, but a slow first frame here passes that before layout,
        // so OK takes autoFocus. A click on the barrier clears focus, so Escape goes first.
        updates(&mut app, 4);
        press(&mut app, "Escape");
        updates(&mut app, 4);
        // Bevy 0.20 asks on Escape's release as well as its press.
        let n = said(&app, "close").len();
        assert!(n >= 1 && said(&app, "close").iter().all(|l| l == "close true"), "the barrier names itself: {:?}", said(&app, "close"));
        click(&mut app, 200.0, 140.0);
        assert_eq!(said(&app, "close").len(), n, "a click on the dialog does not close it");
        click(&mut app, 10.0, 10.0);
        assert_eq!(said(&app, "close").len(), n + 1, "a click on the barrier does");
    }

    #[test]
    fn a_dialog_moves_by_its_handle() {
        let _hang = common::watchdog("a_dialog_moves_by_its_handle", common::BOUND);
        let mut app = start("ui-overlays-drag", MODAL);
        send(&mut app, &[pointer("move", 200.0, 110.0), pointer("down", 200.0, 110.0), pointer("move", 220.0, 120.0), pointer("move", 240.0, 130.0), pointer("up", 240.0, 130.0)]);
        let d = node(&mut app, &["barrier", "d"]);
        assert_eq!(app.world().get::<UiTransform>(d).map(|t| t.translation), Some(Val2::px(40.0, 20.0)));
    }
}
