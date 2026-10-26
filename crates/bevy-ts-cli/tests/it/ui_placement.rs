//! Stage 3b of bevy-ui-coverage: many nodes placed by a bound `transform`, not `style`. The
//! binding writes `UiTransform` and never `Node`, so taffy lays out nothing again, and picking
//! reads `UiGlobalTransform`, so a moved node is hit where it is drawn. Layout and picking need the
//! offscreen image and its camera: macOS (Metal), as `headless_ui.rs`.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::cli::{Display, Engine, base_app};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_tsx::TsxNode;
use bevy_ts_viewport::offscreen::{OffscreenPointerPlugin, Pending};
use common::{messages, project, reload};
use std::sync::Arc;

const N: usize = 1000;

/// 1,000 4-px nodes, each a static `style` and a `transform` bound to the frame count; a button
/// drawn 400 px right and 200 px down of its layout place; and, below the nodes, a 100-px clipping
/// field with a button moved 50 px right (inside it) and one moved 150 px right (past its edge).
const GAME: &str = r#"import { app, signal, px, Update } from "bevy-ts";
export const [frame, setFrame] = signal(0);
export function tick() { setFrame(frame() + 1); }
app.addSystems(Update, tick);
app.scene("ui", () => (
  <>
    {Array.from({ length: 1000 }, (_, i) => (
      <node key={`n${i}`} style={{ position: "absolute", left: 0, top: 0, width: px(4), height: px(4) }}
        transform={() => ({ translation: [px((i + frame()) % 900), px(i % 500)] })} />
    ))}
    <button key="moved" style={{ position: "absolute", left: 0, top: 0, width: px(40), height: px(40) }}
      transform={{ translation: [px(400), px(200)] }} onPointerClick={() => console.log("click moved")} />
    <node key="field" style={{ position: "absolute", left: 0, top: px(510), width: px(100), height: px(30), overflow: "clip" }}>
      <button key="inside" style={{ position: "absolute", left: 0, top: 0, width: px(20), height: px(20) }}
        transform={{ translation: [px(50), px(0)] }} onPointerClick={() => console.log("click inside")} />
      <button key="outside" style={{ position: "absolute", left: 0, top: 0, width: px(20), height: px(20) }}
        transform={{ translation: [px(150), px(0)] }} onPointerClick={() => console.log("click outside")} />
    </node>
  </>
));
"#;

fn scene(name: &str) -> App {
    let dir = project(name, &[("game.tsx", GAME)]);
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let mut app = base_app(&engine, bevy_ts_cli::Prebuilt, Display::Headless, Default::default(), None, true, Default::default(), &[], false);
    let image = app.world().resource::<bevy_ts_cli::headless::Offscreen>().0.clone();
    app.add_plugins(OffscreenPointerPlugin { image });
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.tsx"), true).unwrap())));
    app.finish();
    app.cleanup();
    app.world_mut().spawn(Camera2d);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    common::updates(&mut app, 5);
    app
}

fn node(app: &mut App, key: &str) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == [key]).map(|(e, _)| e).unwrap_or_else(|| panic!("no node {key}"))
}

/// Each moved node's `Node` change tick, `ComputedNode` size and drawn centre.
fn sample(app: &App, nodes: &[Entity]) -> Vec<(u32, Vec2, Vec2)> {
    nodes
        .iter()
        .map(|&e| {
            let r = app.world().entity(e);
            (r.get_change_ticks::<Node>().unwrap().changed.get(), r.get::<ComputedNode>().unwrap().size, r.get::<UiGlobalTransform>().unwrap().translation)
        })
        .collect()
}

#[test]
fn a_thousand_nodes_moved_by_transform_never_change_node() {
    let _hang = common::watchdog("a_thousand_nodes_moved_by_transform_never_change_node", common::BOUND);
    let mut app = scene("ui-placement-many");
    let nodes: Vec<Entity> = (0..N).map(|i| node(&mut app, &format!("n{i}"))).collect();
    let before = sample(&app, &nodes);
    common::updates(&mut app, 3);
    let after = sample(&app, &nodes);
    for (i, ((tick0, size0, at0), (tick1, size1, at1))) in before.iter().zip(&after).enumerate() {
        assert_eq!(tick0, tick1, "node {i}: Node changed");
        assert_eq!((*size0, *size1), (Vec2::splat(4.0), Vec2::splat(4.0)), "node {i}: laid out again");
        // Three frames, one pixel each, unless it wrapped at 900.
        let dx = at1.x - at0.x;
        assert!(dx == 3.0 || dx < -800.0, "node {i}: drawn at {at0} then {at1}");
        assert_eq!(at1.y, (i % 500) as f32 + 2.0, "node {i}");
    }
}

/// Clicks at `(x, y)` and counts the `said` messages.
fn click(app: &mut App, x: f64, y: f64, said: &str) -> usize {
    use bevy_ts_remote::protocol::PointerEventKind::{Down, Move, Up};
    use bevy_ts_remote::protocol::{PointerEvent, PointerEventButton, PointerEventType};
    let events = [Move, Down, Up].map(|kind| PointerEvent {
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
    });
    app.world_mut().resource_mut::<Pending>().0.extend(events);
    common::updates(app, 6);
    messages(app).into_iter().filter(|m| m == said).count()
}

#[test]
fn a_button_moved_by_transform_is_clicked_where_it_is_drawn() {
    let _hang = common::watchdog("a_button_moved_by_transform_is_clicked_where_it_is_drawn", common::BOUND);
    let mut app = scene("ui-placement-click");
    let moved = node(&mut app, "moved");
    assert_eq!(app.world().get::<UiGlobalTransform>(moved).unwrap().translation, Vec2::new(420.0, 220.0));
    assert_eq!(click(&mut app, 20.0, 20.0, "click moved"), 0, "its layout place is empty");
    assert_eq!(click(&mut app, 420.0, 220.0, "click moved"), 1, "its drawn place takes the click");
}

#[test]
fn a_button_moved_past_its_clip_is_clipped_where_it_is_drawn() {
    let _hang = common::watchdog("a_button_moved_past_its_clip_is_clipped_where_it_is_drawn", common::BOUND);
    let mut app = scene("ui-placement-clip");
    let outside = app.world_mut().query::<(Entity, &TsxNode)>().iter(app.world()).find(|(_, n)| n.key_path == ["field", "outside"]).map(|(e, _)| e).unwrap();
    assert_eq!(app.world().get::<UiGlobalTransform>(outside).unwrap().translation, Vec2::new(160.0, 520.0), "drawn past the edge");
    assert_eq!(click(&mut app, 60.0, 520.0, "click inside"), 1, "moved inside the field: hit");
    assert_eq!(click(&mut app, 160.0, 520.0, "click outside"), 0, "moved past the field's edge: clipped");
}
