//! Viewport mode against the in-process test viewer: attach and detach, resize, input, stats.
//! Each test drives `App::update` itself; the tick runner has its own unit tests.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::input::keyboard::KeyCode;
use bevy::picking::events::PointerClick;
use bevy::prelude::*;
use bevy::window::{WindowFocused, WindowScaleFactorChanged};
use bevy_ts_viewport::mach::Viewer;
use common::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;


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

#[test]
fn hello_is_answered_and_a_second_hello_replaces_the_first() {
    let mut app = app();
    let (service, viewer1) = attach(&mut app, 400.0, 300.0, 2.0);
    let mut seen1 = Seen::default();
    run_until(&mut app, &viewer1, &mut seen1, LONG, |s| !s.frames.is_empty());
    let hello = seen1.other.iter().find(|(m, _)| m == "hello").expect("hello is answered");
    assert_eq!(hello.1["inputs"], json!(bevy_ts_viewport::input::INPUTS));
    let who = json!({ "kind": "stream", "device": "iPhone", "mobile": true });
    let viewer2 = Viewer::connect_as(&service, who.clone()).unwrap();
    let mut seen2 = Seen::default();
    run_until(&mut app, &viewer2, &mut seen2, LONG, |s| !s.frames.is_empty());
    assert_eq!(seen2.surfaces.len(), 3, "the new viewer is sent the ring again");
    assert!(seen2.other.iter().any(|(m, p)| m == "hello" && p["inputs"].is_array()));
    pump(&viewer1, &mut seen1, Duration::from_millis(20));
    assert!(seen1.other.contains(&("replaced".to_string(), json!({ "viewer": who }))), "{:?}", seen1.other);
    let n = seen1.frames.len();
    for _ in 0..3 {
        app.update();
    }
    pump(&viewer1, &mut seen1, Duration::from_millis(20));
    assert_eq!(seen1.frames.len(), n, "the replaced viewer gets nothing more");
    let stats = call(&mut app, "bevy-ts/viewport_stats", Value::Null).unwrap();
    assert_eq!(stats["attached"], true);
}

#[test]
fn a_viewers_gamepad_reaches_the_game_with_focus() {
    use bevy::input::gamepad::{GamepadAxis, GamepadButton};
    let mut app = app();
    let pads = Pads::default();
    app.insert_resource(pads.clone()).add_systems(Update, |q: Query<&Gamepad>, log: Res<Pads>| {
        for g in &q {
            if g.just_pressed(GamepadButton::South) {
                log.0.lock().unwrap().push("south".into());
            }
            if g.get(GamepadAxis::LeftStickY).is_some_and(|y| y > 0.9) {
                log.0.lock().unwrap().push("up".into());
            }
        }
    });
    let (_, viewer) = attach(&mut app, 400.0, 300.0, 2.0);
    let pad = |kind: &str, south: f64, y: f64| {
        json!({ "type": "gamepad", "kind": kind, "index": 0, "id": "pad",
                "buttons": [{ "pressed": south > 0.5, "value": south }], "axes": [0.0, y] })
    };
    let send = |app: &mut App, e: Value| {
        viewer.send("input", e).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        app.update();
    };
    send(&mut app, pad("connected", 0.0, 0.0));
    app.update();
    assert_eq!(app.world_mut().query::<&Gamepad>().iter(app.world()).count(), 1, "the connection passes the gate");
    send(&mut app, pad("state", 1.0, 0.0));
    assert!(pads.0.lock().unwrap().is_empty(), "no focus, no gamepad");
    send(&mut app, json!({ "type": "focus", "on": true }));
    send(&mut app, pad("state", 0.0, 0.0));
    send(&mut app, pad("state", 1.0, -1.0));
    assert_eq!(*pads.0.lock().unwrap(), vec!["south", "up"]);
    send(&mut app, json!({ "type": "gamepad", "kind": "disconnected", "index": 0 }));
    app.update();
    assert_eq!(app.world_mut().query::<&Gamepad>().iter(app.world()).count(), 0);
}

/// `bevy-ts/screenshot` under the viewport: Bevy cannot capture the virtual window, which has no
/// wgpu surface, so the ring reads its slot back (change `brp-screenshot`).
#[test]
fn a_screenshot_is_the_frame_the_viewport_shows() {
    let mut app = app_with(bevy_ts_remote::add_methods);
    app.insert_resource(ClearColor(Color::srgb(0.9, 0.3, 0.1)));
    let (_, viewer) = attach(&mut app, 400.0, 300.0, 2.0);
    let mut seen = Seen::default();
    run_until(&mut app, &viewer, &mut seen, LONG, |s| s.frames.len() >= 2);
    let dir = std::env::temp_dir().join(format!("bevy-ts-viewport-shot-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.canonicalize().unwrap().join("viewport.png");
    let _ = std::fs::remove_file(&path);
    let id = match app.world().resource::<bevy::remote::RemoteMethods>().get("bevy-ts/screenshot") {
        Some(bevy::remote::RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("bevy-ts/screenshot is a deferred method"),
    };
    let params = Some(json!({ "path": path }));
    let t0 = std::time::Instant::now();
    let r = loop {
        assert!(t0.elapsed() < LONG, "no answer");
        if let Some(r) = app.world_mut().run_system_with(id, params.clone()).unwrap().transpose() {
            break r;
        }
        app.update();
        pump(&viewer, &mut seen, Duration::from_millis(2));
    };
    let r = r.unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(800), Some(600)), "{r}");
    let bytes = std::fs::read(&path).unwrap();
    let image = Image::from_buffer(
        &bytes,
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::NONE,
        true,
        bevy::image::ImageSampler::Default,
        bevy::asset::RenderAssetUsages::default(),
    )
    .unwrap();
    assert_eq!((image.width(), image.height()), (800, 600));
    // A corner shows the clear colour, past the cube.
    let corner = image.get_color_at(4, 4).unwrap().to_srgba();
    assert!((corner.red - 0.9).abs() < 0.02 && (corner.blue - 0.1).abs() < 0.02, "{corner:?}");
}
