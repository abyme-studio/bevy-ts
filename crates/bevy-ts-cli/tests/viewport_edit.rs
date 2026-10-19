//! Change `viewport`, task 6.4: the round trip with a scripted editor over real TSX. The app is
//! the one `bevy-ts dev --viewport` builds (no window, the virtual one), driven by protocol
//! calls; the "editor" rewrites `game.tsx` the way abyme's would.
#![cfg(target_os = "macos")]

mod common;

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_tsx::TsxNode;
use bevy_ts_v8::V8Options;
use bevy_ts_viewport::edit::EditorCamera;
use common::{project, reload, write};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const ENEMY: &str = r##"import { Transform } from "bevy-ts/bevy";

export const Enemy = ({ at }: { at: [number, number, number] }) => (
  <entity name="enemy">
    <Transform translation={at} />
    <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
    <entity key="eye"><Transform translation={[0, 0.6, 0.5]} /></entity>
  </entity>
);
"##;

fn game(b_at: &str) -> String {
    format!(
        "import {{ app }} from \"bevy-ts\";\nimport {{ Enemy }} from \"./enemy.tsx\";\n\nconst spawn = (i: number): [number, number, number] => [i * -3, 0, 0];\n\napp.scene(\"arena\", () => (\n  <>\n    <Enemy key=\"b\" at={{{b_at}}} />\n    <Enemy key=\"c\" at={{spawn(2)}} />\n  </>\n));\n"
    )
}

const B_LINE: u32 = 8;

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
    .add_plugins(bevy_ts_tsx::TsxPlugin);
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
    let params = (!params.is_null()).then_some(params);
    app.world_mut().run_system_with(id, params).unwrap().unwrap_or_else(|e| panic!("{method}: {}", e.message))
}

fn watch(app: &mut App, method: &str, key: u64) -> Option<Value> {
    let id = match app.world().resource::<RemoteMethods>().get(method) {
        Some(RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("{method} not served"),
    };
    app.world_mut().run_system_with(id, Some(json!({ "k": key }))).unwrap().unwrap()
}

fn updates(app: &mut App, n: usize) {
    for _ in 0..n {
        std::thread::sleep(Duration::from_millis(5));
        app.update();
    }
}

fn input(app: &mut App, events: Value) {
    call(app, "bevy-ts/viewport_input", json!({ "events": events }));
}

fn enemy(app: &mut App, key: &str) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &TsxNode)>().iter(world).find(|(_, n)| n.key_path == [key]).map(|(e, _)| e).unwrap()
}

fn screen(app: &mut App, p: Vec3) -> Vec2 {
    let world = app.world_mut();
    let (c, gt) = world.query_filtered::<(&Camera, &GlobalTransform), With<EditorCamera>>().single(world).unwrap();
    c.world_to_viewport(gt, p).unwrap()
}

/// Drags the primary selection's translate X handle by `dx` points; answers the edits stream.
fn drag_x(app: &mut App, e: Entity, dx: f32) {
    let pos = app.world().get::<GlobalTransform>(e).unwrap().translation();
    let cam = app.world_mut().query_filtered::<&GlobalTransform, With<EditorCamera>>().single(app.world()).unwrap().translation();
    let handle = screen(app, pos + Vec3::X * 0.6 * (cam - pos).length() * 0.1);
    input(app, json!([{ "type": "pointer", "kind": "move", "x": handle.x, "y": handle.y }]));
    updates(app, 2);
    input(app, json!([{ "type": "pointer", "kind": "down", "x": handle.x, "y": handle.y, "button": "left" }]));
    updates(app, 1);
    for i in 1..=5 {
        input(app, json!([{ "type": "pointer", "kind": "move", "x": handle.x + dx * i as f32 / 5.0, "y": handle.y }]));
        updates(app, 1);
    }
    input(app, json!([{ "type": "pointer", "kind": "up", "x": handle.x + dx, "y": handle.y, "button": "left" }]));
    updates(app, 2);
}

fn fmt(v: f64) -> String {
    format!("{:.3}", v).trim_end_matches('0').trim_end_matches('.').to_string()
}

#[test]
fn a_literal_drag_is_written_by_the_editor_and_a_computed_one_rejected() {
    let dir = project("viewport-edit-roundtrip", &[("enemy.tsx", ENEMY), ("game.tsx", &game("[3, 0, -4]"))]);
    let mut app = viewport_app(&dir.join("game.tsx"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    updates(&mut app, 3);
    let r = call(&mut app, "bevy-ts/viewport_mode", json!({ "mode": "edit" }));
    assert_eq!(r["paused"], true);
    updates(&mut app, 2);

    // The editor selects b by its line in game.tsx.
    let sel = call(&mut app, "bevy-ts/viewport_select", json!({ "file": "game.tsx", "line": B_LINE }));
    let b = enemy(&mut app, "b");
    assert_eq!(sel["origin"], "editor");
    assert_eq!(sel["items"][0]["entity"], b.to_bits(), "{sel:#}");
    assert_eq!(sel["items"][0]["routes"]["translation"]["kind"], "tsx");
    updates(&mut app, 2);

    drag_x(&mut app, b, 80.0);
    let edits = watch(&mut app, "bevy-ts/viewport_edits+watch", 1).unwrap();
    let edit = edits["edits"][0].clone();
    assert_eq!(edits["edits"].as_array().unwrap().len(), 1, "{edits:#}");
    assert_eq!(edit["route"]["kind"], "tsx");
    let chain = edit["route"]["source"].as_array().unwrap();
    assert!(chain.last().unwrap()["file"].as_str().unwrap().ends_with("game.tsx"));
    assert_eq!(chain.last().unwrap()["line"], B_LINE);
    assert_eq!(edit["before"]["translation"][0], 3.0);
    let new_x = edit["after"]["translation"][0].as_f64().unwrap();
    assert!(new_x > 3.2, "{edit:#}");

    // The editor: the call site's `at` is a literal, so it rewrites it and answers.
    let src = game(&format!("[{}, 0, -4]", fmt(new_x)));
    write(&dir, "game.tsx", &src);
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    call(&mut app, "bevy-ts/viewport_edit_result", json!({ "id": edit["id"], "status": "written" }));
    updates(&mut app, 2);
    let b2 = enemy(&mut app, "b");
    let x = app.world().get::<Transform>(b2).unwrap().translation.x as f64;
    assert!((x - new_x).abs() < 1e-3, "no jump after the reload: {x} vs {new_x}");
    let sel = watch(&mut app, "bevy-ts/viewport_selection+watch", 2).unwrap();
    assert_eq!(sel["items"][0]["entity"], b2.to_bits(), "b is still selected");
    let mode = call(&mut app, "bevy-ts/viewport_mode", Value::Null);
    assert_eq!(mode["mode"], "edit", "the reload kept the mode");
    let edits = watch(&mut app, "bevy-ts/viewport_edits+watch", 3).unwrap();
    assert_eq!(edits["edits"][0]["status"], "written");

    // c's `at` is computed at its call site: the editor rejects, the engine restores and locks.
    let c = enemy(&mut app, "c");
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [c.to_bits()] }));
    updates(&mut app, 2);
    let c_x = app.world().get::<Transform>(c).unwrap().translation.x;
    drag_x(&mut app, c, 80.0);
    assert!(app.world().get::<Transform>(c).unwrap().translation.x > c_x + 0.2);
    let edits = watch(&mut app, "bevy-ts/viewport_edits+watch", 4).unwrap();
    let edit = edits["edits"].as_array().unwrap().last().unwrap().clone();
    call(&mut app, "bevy-ts/viewport_edit_result", json!({ "id": edit["id"], "status": "rejected", "reason": "computed" }));
    updates(&mut app, 1);
    assert_eq!(app.world().get::<Transform>(c).unwrap().translation.x, c_x, "restored within a frame");
    let sel = watch(&mut app, "bevy-ts/viewport_selection+watch", 5).unwrap();
    assert_eq!(sel["items"][0]["routes"]["translation"]["kind"], "locked");
    assert_eq!(sel["items"][0]["routes"]["translation"]["reason"], "computed");
}
