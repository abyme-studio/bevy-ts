//! Viewport mode against the in-process test viewer: attach and detach, resize, input, stats.
//! Each test drives `App::update` itself; the tick runner has its own unit tests.
#![cfg(target_os = "macos")]

use bevy::input::keyboard::KeyCode;
use bevy::picking::events::PointerClick;
use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::window::{ExitCondition, WindowFocused, WindowScaleFactorChanged};
use bevy::winit::WinitPlugin;
use bevy_ts_viewport::ViewportPlugin;
use bevy_ts_viewport::mach::{Incoming, Viewer, release_port};
use bevy_ts_viewport::surface::Surface;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn app() -> App {
    app_with(|_| {})
}

/// An app with extra plugins added before it is finished.
fn app_with(extra: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(RemotePlugin::default())
    .add_plugins(ViewportPlugin::default())
    .add_systems(Startup, scene);
    extra(&mut app);
    app.finish();
    app.cleanup();
    app
}

#[derive(Component)]
struct TheCube;

fn scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((DirectionalLight::default(), Transform::from_xyz(3.0, 6.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((TheCube, Mesh3d(meshes.add(Cuboid::from_size(Vec3::splat(2.0)))), MeshMaterial3d(materials.add(StandardMaterial::default()))));
}

fn rewrite(v: &mut Value) {
    match v {
        Value::Object(o) => {
            if let Some(Value::String(r)) = o.get_mut("$ref") {
                *r = r.replace("#/components/schemas/", "#/definitions/");
            }
            o.values_mut().for_each(rewrite);
        }
        Value::Array(a) => a.iter_mut().for_each(rewrite),
        _ => {}
    }
}

/// Validates a method's result against `protocol/bevy-ts.openrpc.json`.
fn validate(method: &str, result: &Value) {
    let doc: Value = serde_json::from_str(bevy_ts_remote::OPENRPC).unwrap();
    let m = doc["methods"].as_array().unwrap().iter().find(|m| m["name"] == method).unwrap();
    assert_eq!(m["x-bevy-ts-crate"], "bevy-ts-viewport");
    let mut schema = m["result"]["schema"].clone();
    let mut defs = doc["components"]["schemas"].clone();
    rewrite(&mut schema);
    rewrite(&mut defs);
    let mut root = json!({ "$schema": "http://json-schema.org/draft-07/schema#", "definitions": defs });
    if let Value::Object(s) = schema {
        root.as_object_mut().unwrap().extend(s);
    }
    let v = jsonschema::draft7::new(&root).unwrap();
    let errors: Vec<String> = v.iter_errors(result).map(|e| format!("{} at {}", e, e.instance_path)).collect();
    assert!(errors.is_empty(), "{method} answered off-contract: {errors:?}\n{result:#}");
}

fn call(app: &mut App, method: &str, params: Value) -> Result<Value, String> {
    let id = match app.world().resource::<RemoteMethods>().get(method) {
        Some(RemoteMethodSystemId::Instant(id)) => *id,
        _ => return Err(format!("no method {method}")),
    };
    let params = (!params.is_null()).then_some(params);
    let r = app.world_mut().run_system_with(id, params).unwrap().map_err(|e| e.message)?;
    validate(method, &r);
    Ok(r)
}

/// What a viewer has received.
#[derive(Default)]
struct Seen {
    surfaces: BTreeMap<(u64, u64), Surface>,
    frames: Vec<Value>,
    other: Vec<(String, Value)>,
}

fn pump(viewer: &Viewer, seen: &mut Seen, wait: Duration) {
    while let Some(m) = viewer.recv(wait) {
        let Incoming::Message { method, params, port } = m else { continue };
        match method.as_str() {
            "surface" => {
                let port = port.unwrap();
                let s = Surface::from_mach_port(port).unwrap();
                release_port(port);
                seen.surfaces.insert((params["ring"].as_u64().unwrap(), params["slot"].as_u64().unwrap()), s);
            }
            "frame" => seen.frames.push(params),
            _ => seen.other.push((method, params)),
        }
    }
}

/// Updates until `done` or the timeout (pipelines compile on the first frames).
fn run_until(app: &mut App, viewer: &Viewer, seen: &mut Seen, timeout: Duration, mut done: impl FnMut(&Seen) -> bool) {
    let t0 = Instant::now();
    while !done(seen) {
        assert!(t0.elapsed() < timeout, "timed out");
        app.update();
        pump(viewer, seen, Duration::from_millis(2));
    }
}

fn attach(app: &mut App, w: f64, h: f64, scale: f64) -> (String, Viewer) {
    let r = call(app, "bevy-ts/viewport_attach", json!({ "w": w, "h": h, "scale": scale, "hz": 120 })).unwrap();
    let service = r["service"].as_str().unwrap().to_string();
    assert!(service.starts_with(&format!("dev.bevy-ts.viewport.{}.", std::process::id())));
    assert_eq!(r["protocol"], bevy_ts_remote::protocol::VERSION);
    let viewer = Viewer::connect(&service).unwrap();
    (service, viewer)
}

const LONG: Duration = Duration::from_secs(60);

#[test]
fn attach_sends_three_surfaces_then_frames() {
    let mut app = app();
    let (_, viewer) = attach(&mut app, 800.0, 600.0, 2.0);
    let mut seen = Seen::default();
    run_until(&mut app, &viewer, &mut seen, LONG, |s| s.frames.len() >= 3);
    assert_eq!(seen.surfaces.len(), 3);
    for s in seen.surfaces.values() {
        assert!(s.width() >= 1600 && s.height() >= 1200);
    }
    let f = &seen.frames[0];
    assert_eq!((f["w"].as_u64(), f["h"].as_u64()), (Some(1600), Some(1200)));
    assert!(f["ts"].as_u64().unwrap() > 0);
    let stats = call(&mut app, "bevy-ts/viewport_stats", Value::Null).unwrap();
    assert_eq!(stats["attached"], true);
    assert!(stats["frames"].as_u64().unwrap() >= 3);
    assert_eq!(stats["size"]["pixelW"], 1600);
}

#[test]
fn a_viewer_that_quits_is_detached_and_the_game_keeps_running() {
    let mut app = app();
    let (_, viewer) = attach(&mut app, 400.0, 300.0, 2.0);
    let mut seen = Seen::default();
    run_until(&mut app, &viewer, &mut seen, LONG, |s| !s.frames.is_empty());
    drop(viewer);
    // The dead-name notification reaches the engine's receive thread, then the next frame.
    std::thread::sleep(Duration::from_millis(50));
    app.update();
    app.update();
    let before = call(&mut app, "bevy-ts/viewport_stats", Value::Null).unwrap();
    assert_eq!(before["attached"], false);
    let update0 = app.world().resource::<bevy_ts_viewport::UpdateCount>().0;
    for _ in 0..5 {
        app.update();
    }
    std::thread::sleep(Duration::from_millis(50));
    let after = call(&mut app, "bevy-ts/viewport_stats", Value::Null).unwrap();
    assert_eq!(before["frames"], after["frames"], "no frames after detach");
    assert_eq!(app.world().resource::<bevy_ts_viewport::UpdateCount>().0, update0 + 5);
}

#[test]
fn a_second_attach_replaces_the_first() {
    let mut app = app();
    let (first, viewer1) = attach(&mut app, 400.0, 300.0, 2.0);
    let mut seen1 = Seen::default();
    run_until(&mut app, &viewer1, &mut seen1, LONG, |s| !s.frames.is_empty());
    let (second, viewer2) = attach(&mut app, 400.0, 300.0, 2.0);
    assert_ne!(first, second);
    assert!(Viewer::connect(&first).is_err(), "the first service is ended");
    let mut seen2 = Seen::default();
    run_until(&mut app, &viewer2, &mut seen2, LONG, |s| s.frames.len() >= 2);
    assert_eq!(seen2.surfaces.len(), 3);
    let n = seen1.frames.len();
    pump(&viewer1, &mut seen1, Duration::from_millis(20));
    assert!(seen1.frames.len() <= n + 1, "the first viewer gets nothing more");
    let r = call(&mut app, "bevy-ts/viewport_detach", Value::Null).unwrap();
    assert_eq!(r["detached"], true);
}

#[test]
fn a_resize_storm_makes_at_most_one_new_ring_and_frames_match_the_size() {
    let mut app = app();
    let (_, viewer) = attach(&mut app, 800.0, 600.0, 2.0);
    let mut seen = Seen::default();
    run_until(&mut app, &viewer, &mut seen, LONG, |s| !s.frames.is_empty());
    for i in 0..30 {
        let w = 800.0 + (i + 1) as f64 * 100.0 / 30.0;
        viewer.send("resize", json!({ "w": w, "h": 600, "scale": 2 })).unwrap();
        app.update();
        pump(&viewer, &mut seen, Duration::from_millis(1));
    }
    run_until(&mut app, &viewer, &mut seen, LONG, |s| s.frames.last().is_some_and(|f| f["w"] == 1800));
    let rings: std::collections::BTreeSet<_> = seen.frames.iter().map(|f| f["ring"].as_u64().unwrap()).collect();
    assert!(rings.len() <= 2, "at most one new ring: {rings:?}");
    // Every surface of the frame's ring holds the frame's size.
    for f in &seen.frames {
        let s = &seen.surfaces[&(f["ring"].as_u64().unwrap(), f["slot"].as_u64().unwrap())];
        assert!(s.width() as u64 >= f["w"].as_u64().unwrap());
    }
    // Widths only grow, never stale.
    assert!(seen.frames.windows(2).all(|w| w[0]["w"].as_u64() <= w[1]["w"].as_u64()));
}

#[derive(Resource, Default)]
struct Log(Vec<String>);

#[test]
fn a_scale_change_sends_scale_factor_changed_and_frames_at_the_new_size() {
    let mut app = app();
    app.init_resource::<Log>().add_systems(Update, |mut r: MessageReader<WindowScaleFactorChanged>, mut log: ResMut<Log>| {
        for e in r.read() {
            log.0.push(format!("scale {}", e.scale_factor));
        }
    });
    let (_, viewer) = attach(&mut app, 400.0, 300.0, 2.0);
    let mut seen = Seen::default();
    run_until(&mut app, &viewer, &mut seen, LONG, |s| !s.frames.is_empty());
    viewer.send("resize", json!({ "w": 400, "h": 300, "scale": 1 })).unwrap();
    run_until(&mut app, &viewer, &mut seen, LONG, |s| s.frames.last().is_some_and(|f| f["w"] == 400));
    assert_eq!(app.world().resource::<Log>().0, vec!["scale 1"]);
    let f = seen.frames.last().unwrap();
    assert_eq!((f["w"].as_u64(), f["h"].as_u64()), (Some(400), Some(300)));
}

#[derive(Resource, Default, Clone)]
struct Keys(Arc<Mutex<Vec<String>>>);

fn record_keys(keys: Res<ButtonInput<KeyCode>>, log: Res<Keys>, mut focus: MessageReader<WindowFocused>) {
    let mut l = log.0.lock().unwrap();
    if keys.just_pressed(KeyCode::KeyW) {
        l.push("W pressed".into());
    }
    if keys.just_released(KeyCode::KeyW) {
        l.push("W released".into());
    }
    for f in focus.read() {
        l.push(format!("focused {}", f.focused));
    }
}

#[test]
fn keys_over_http_reach_button_input() {
    let mut app = app();
    let log = Keys::default();
    app.insert_resource(log.clone()).add_systems(Update, record_keys);
    call(&mut app, "bevy-ts/viewport_attach", json!({ "w": 400, "h": 300, "scale": 2 })).unwrap();
    let r = call(&mut app, "bevy-ts/viewport_input", json!({ "events": [{ "type": "key", "state": "down", "code": "KeyW", "key": "w", "text": "w" }] })).unwrap();
    assert_eq!(r["queued"], 1);
    app.update();
    call(&mut app, "bevy-ts/viewport_input", json!({ "events": [{ "type": "key", "state": "up", "code": "KeyW", "key": "w" }] })).unwrap();
    app.update();
    assert_eq!(*log.0.lock().unwrap(), vec!["W pressed", "W released"]);
}

#[test]
fn focus_loss_releases_held_keys() {
    let mut app = app();
    let log = Keys::default();
    app.insert_resource(log.clone()).add_systems(Update, record_keys);
    let (_, viewer) = attach(&mut app, 400.0, 300.0, 2.0);
    viewer.send("input", json!({ "type": "focus", "on": true })).unwrap();
    viewer.send("input", json!({ "type": "key", "state": "down", "code": "KeyW" })).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    app.update();
    viewer.send("input", json!({ "type": "focus", "on": false })).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    app.update();
    assert_eq!(*log.0.lock().unwrap(), vec!["W pressed", "focused true", "W released", "focused false"]);
}

#[derive(Resource, Default, Clone)]
struct Clicks(Arc<Mutex<Vec<String>>>);

#[test]
fn a_click_on_the_cube_fires_its_pointer_click_observer_once() {
    let mut app = app_with(|app| {
        app.add_plugins(bevy::picking::mesh_picking::MeshPickingPlugin);
    });
    let clicks = Clicks::default();
    app.insert_resource(clicks.clone());
    app.add_systems(PostStartup, |cubes: Query<Entity, With<TheCube>>, mut commands: Commands| {
        for e in &cubes {
            commands.entity(e).observe(|_: On<PointerClick>, c: Res<Clicks>| c.0.lock().unwrap().push("click".into()));
        }
    });
    let (_, viewer) = attach(&mut app, 400.0, 300.0, 2.0);
    let mut seen = Seen::default();
    run_until(&mut app, &viewer, &mut seen, LONG, |s| !s.frames.is_empty());
    // The cube is in the middle of the hole.
    // As a real pointer does: it hovers before it presses.
    viewer.send("input", json!({ "type": "pointer", "kind": "move", "x": 200, "y": 150 })).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    for _ in 0..3 {
        app.update();
    }
    viewer.send("input", json!({ "type": "pointer", "kind": "down", "x": 200, "y": 150, "button": "left", "clicks": 1 })).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    app.update();
    app.update();
    viewer.send("input", json!({ "type": "pointer", "kind": "up", "x": 200, "y": 150, "button": "left", "clicks": 1 })).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(*clicks.0.lock().unwrap(), vec!["click"]);
    // An array is not one event: rejected, nothing applied.
    viewer.send("input", json!([{ "type": "pointer", "kind": "down", "x": 200, "y": 150 }])).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    app.update();
    assert!(app.world().resource::<bevy_ts_viewport::input::PendingInput>().0.is_empty());
    let stats = call(&mut app, "bevy-ts/viewport_stats", Value::Null).unwrap();
    assert!(stats["inputToFrame"]["samples"].as_u64().unwrap() > 0, "{stats}");
}

#[derive(Resource, Default, Clone)]
struct Pads(Arc<Mutex<Vec<String>>>);

#[test]
fn gamepad_input_waits_for_focus() {
    use bevy::input::gamepad::{GamepadButton, GamepadConnection, GamepadConnectionEvent, RawGamepadButtonChangedEvent, RawGamepadEvent};
    let mut app = app();
    let pads = Pads::default();
    app.insert_resource(pads.clone()).add_systems(Update, |q: Query<&Gamepad>, log: Res<Pads>| {
        for g in &q {
            if g.just_pressed(GamepadButton::South) {
                log.0.lock().unwrap().push("south".into());
            }
        }
    });
    let (_, viewer) = attach(&mut app, 400.0, 300.0, 2.0);
    let pad = app.world_mut().spawn_empty().id();
    let connect = GamepadConnectionEvent::new(pad, GamepadConnection::Connected { name: "pad".into(), vendor_id: None, product_id: None });
    // As gilrs does: the connection both raw and as itself.
    app.world_mut().write_message(RawGamepadEvent::Connection(connect.clone()));
    app.world_mut().write_message(connect);
    app.update();
    assert!(app.world().get::<Gamepad>(pad).is_some(), "the connection passes the gate");
    let press = |app: &mut App, v: f32| {
        app.world_mut().write_message(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(pad, GamepadButton::South, v)));
        app.update();
    };
    press(&mut app, 1.0);
    press(&mut app, 0.0);
    assert!(pads.0.lock().unwrap().is_empty(), "no focus, no gamepad");
    viewer.send("input", json!({ "type": "focus", "on": true })).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    app.update();
    assert!(app.world().resource::<bevy_ts_viewport::shared::SharedRes>().0.focused());
    press(&mut app, 1.0);
    let pressed = app.world().get::<Gamepad>(pad).unwrap().pressed(GamepadButton::South);
    assert_eq!(*pads.0.lock().unwrap(), vec!["south"], "pressed now: {pressed}");
}
