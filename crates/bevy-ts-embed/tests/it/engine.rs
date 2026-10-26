//! Change android-native, engine-embedding: a headless engine driven through the C ABI on a built
//! game, and, where a test needs to see inside the world, through the Rust `Engine` with a setup
//! hook the C ABI never offers.

use crate::common::{config, create, game, one_engine, wait_event, wait_until};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy_ts_embed::engine::{Engine, Surface};
use serde_json::{Value, json};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const HELLO: &str = r#"
import { app } from "bevy-ts";
console.log("hello from the game");
"#;

fn headless() -> Surface {
    Surface { window: None, width: 0, height: 0, scale: 1.0 }
}

/// What a Rust system in the engine saw, frame by frame.
#[derive(Default)]
struct Seen {
    frames: u32,
    deltas: Vec<Duration>,
    touches: Vec<(u64, TouchPhase, Vec2)>,
    keys: Vec<KeyCode>,
}

#[derive(Resource, Clone)]
struct Shared(Arc<Mutex<Seen>>);

fn watch(shared: Res<Shared>, time: Res<Time<Virtual>>, mut touches: MessageReader<TouchInput>, keys: Option<Res<ButtonInput<KeyCode>>>) {
    let mut seen = shared.0.lock().unwrap();
    seen.frames += 1;
    seen.deltas.push(time.delta());
    seen.touches.extend(touches.read().map(|t| (t.id, t.phase, t.position)));
    if let Some(keys) = keys {
        seen.keys.extend(keys.get_just_pressed().copied());
    }
}

/// An engine through the Rust API with a frame watcher; its events and what it saw.
fn watched(config: &str, extra: impl FnOnce(&mut App) + Send + 'static) -> (Engine, Receiver<String>, Arc<Mutex<Seen>>) {
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let seen = Arc::new(Mutex::new(Seen::default()));
    let shared = Shared(seen.clone());
    let setup = Box::new(move |app: &mut App| {
        app.insert_resource(shared).add_systems(Update, watch);
        extra(app);
    });
    let events = Arc::new(move |e: &str| {
        let _ = tx.lock().unwrap().send(e.to_string());
    });
    let engine = Engine::create(config, headless(), events, Some(setup)).unwrap();
    (engine, rx, seen)
}

fn frames(seen: &Arc<Mutex<Seen>>) -> u32 {
    seen.lock().unwrap().frames
}

#[test]
fn create_starts_and_destroy_exits() {
    let _one = one_engine();
    let game = game("embed-start", HELLO, |_| {});
    let engine = create(&config(&game, json!({}))).unwrap();
    let mut seen = wait_event(&engine.events, &["\"started\""]);
    let started: Value = serde_json::from_str(seen.last().unwrap()).unwrap();
    assert_eq!(started["js"], "v8");
    assert!(started["inputs"].as_array().unwrap().iter().any(|i| i == "touch"), "{started}");
    // The first frame's log is sent before `started`, which follows that frame.
    let hello = |e: &String| e.contains("\"log\"") && e.contains("hello from the game");
    if !seen.iter().any(hello) {
        seen = wait_event(&engine.events, &["\"log\"", "hello from the game"]);
    }
    assert!(seen.iter().any(hello), "{seen:?}");
    let after = engine.destroy();
    let exited: Value = serde_json::from_str(after.last().expect("exited before destroy returned")).unwrap();
    assert_eq!(exited, json!({ "event": "exited", "code": 0 }));
}

#[test]
fn another_versions_game_is_refused_and_leaves_no_engine() {
    let _one = one_engine();
    let old = game("embed-old", HELLO, |g| g.bevy_ts = "0.0.1".into());
    let e = create(&config(&old, json!({}))).err().expect("refused");
    assert!(e.contains("0.0.1") && e.contains(bevy_ts_embed::play::VERSION), "{e}");
    // Nothing was left running: an engine can be made right away.
    let ok = game("embed-after-old", HELLO, |_| {});
    let engine = create(&config(&ok, json!({}))).unwrap();
    wait_event(&engine.events, &["\"started\""]);
    engine.destroy();
}

#[test]
fn a_project_engines_game_is_refused() {
    let _one = one_engine();
    let native = game("embed-native", HELLO, |g| g.engine = "abc123".into());
    let e = create(&config(&native, json!({}))).err().expect("refused");
    assert!(e.contains("built for its project engine abc123"), "{e}");
}

#[test]
fn origin_headers_need_the_origin_they_go_to() {
    let _one = one_engine();
    let g = game("embed-origin-headers", HELLO, |_| {});
    let e = create(&config(&g, json!({ "originHeaders": { "authorization": "Bearer k" } }))).err().expect("refused");
    assert!(e.contains("originHeaders needs headersOrigin"), "{e}");
}

#[test]
fn a_second_engine_is_refused_and_the_first_plays_on() {
    let _one = one_engine();
    let game = game("embed-second", HELLO, |_| {});
    let (first, _events, seen) = watched(&config(&game, json!({})), |_| {});
    wait_until("the first plays", || frames(&seen) > 2);
    let e = create(&config(&game, json!({}))).err().expect("refused");
    assert_eq!(e, "an engine is already running");
    let before = frames(&seen);
    wait_until("the first keeps playing", || frames(&seen) > before + 2);
    first.destroy();
}

#[test]
fn pause_stops_frames_and_resume_caps_the_delta() {
    let _one = one_engine();
    let game = game("embed-pause", HELLO, |_| {});
    let (engine, _events, seen) = watched(&config(&game, json!({})), |_| {});
    wait_until("frames run", || frames(&seen) > 2);
    engine.pause();
    // The pause applies at the next frame boundary: wait for the count to settle.
    let mut last = frames(&seen);
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let now = frames(&seen);
        if now == last {
            break;
        }
        last = now;
    }
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(frames(&seen), last, "frames ran while paused");
    engine.resume();
    wait_until("frames run again", || frames(&seen) > last);
    let delta = seen.lock().unwrap().deltas[last as usize];
    let max = Time::<Virtual>::default().max_delta();
    assert!(delta <= max, "the first frame after resume saw {delta:?}, above {max:?}");
    engine.destroy();
}

const SOCKET: &str = r#"
import { app } from "bevy-ts";
const ws = new WebSocket("ws://127.0.0.1:4477/api/stream");
ws.onopen = () => console.log("open");
ws.onmessage = (e) => console.log("message " + e.data);
ws.onclose = (e) => console.log(`close ${e.code}`);
"#;

#[test]
fn a_socket_survives_a_pause() {
    let _one = one_engine();
    let game = game("embed-socket", SOCKET, |g| g.net.allow = vec!["127.0.0.1:4477".into()]);
    let fake = Arc::new(Mutex::new(None));
    let slot = fake.clone();
    let (engine, events, _seen) = watched(&config(&game, json!({})), move |app| {
        *slot.lock().unwrap() = Some(bevy_ts::net::install_fake(app));
    });
    // The setup runs on the engine's thread, after `create` returns.
    wait_until("the engine is set up", || fake.lock().unwrap().is_some());
    let fake = fake.lock().unwrap().clone().unwrap();
    wait_until("the script opens its socket", || fake.last().is_some());
    let (wire, _) = fake.last().unwrap();
    fake.accept(wire, "");
    wait_event(&events, &["\"log\"", "open"]);
    engine.pause();
    std::thread::sleep(Duration::from_millis(200));
    fake.push_text(wire, "during the pause");
    std::thread::sleep(Duration::from_millis(200));
    engine.resume();
    let seen = wait_event(&events, &["message during the pause"]);
    assert!(!seen.iter().any(|e| e.contains("close")), "{seen:?}");
    assert!(fake.conn(wire).unwrap().closed.is_none());
    engine.destroy();
    // Destroy closes it with 1001.
    assert_eq!(fake.conn(wire).unwrap().closed.map(|c| c.0), Some(Some(1001)));
}

#[test]
fn touch_and_key_input_reach_the_game() {
    let _one = one_engine();
    let game = game("embed-input", HELLO, |_| {});
    let (engine, _events, seen) = watched(&config(&game, json!({})), |_| {});
    wait_until("frames run", || frames(&seen) > 1);
    let touch = |kind: &str, touches: Value| json!({ "type": "touch", "kind": kind, "touches": touches }).to_string();
    let send = |e: String| {
        engine.input(e);
    };
    send(touch("start", json!([{ "id": 7, "x": 10.0, "y": 20.0 }])));
    send(touch("end", json!([])));
    send(json!({ "type": "key", "state": "down", "code": "KeyW", "key": "w" }).to_string());
    wait_until("the touch and key arrive", || {
        let s = seen.lock().unwrap();
        s.touches.len() == 2 && s.keys.contains(&KeyCode::KeyW)
    });
    let s = seen.lock().unwrap();
    assert_eq!(s.touches, vec![(7, TouchPhase::Started, Vec2::new(10.0, 20.0)), (7, TouchPhase::Ended, Vec2::new(10.0, 20.0))]);
    drop(s);
    engine.destroy();
}

#[test]
fn malformed_input_is_logged_and_the_game_plays_on() {
    let _one = one_engine();
    let game = game("embed-junk", HELLO, |_| {});
    let engine = create(&config(&game, json!({}))).unwrap();
    wait_event(&engine.events, &["\"started\""]);
    engine.input(r#"{"type":"nope"}"#);
    let seen = wait_event(&engine.events, &["\"warn\"", "malformed input event", "nope"]);
    assert!(!seen.iter().any(|e| e.contains("exited")), "{seen:?}");
    engine.input(r#"{"type":"focus","on":true}"#);
    let after = engine.destroy();
    assert!(after.last().unwrap().contains("\"code\":0"), "{after:?}");
}

const ENV: &str = r#"
import { app } from "bevy-ts";
console.log("env " + JSON.stringify(process.env));
"#;

#[test]
fn the_hosts_env_drives_process_env_and_net_allow() {
    let _one = one_engine();
    let game = game("embed-env", ENV, |g| {
        g.env = vec!["ABYME_URL".into(), "ABYME_VIEW".into()];
        g.net.allow = vec!["${ABYME_URL}".into()];
    });
    let env = json!({ "env": { "ABYME_URL": "https://mac.tail.ts.net:8443", "ABYME_VIEW": "town", "HOME": "/x" } });
    let allowed = Arc::new(Mutex::new(None));
    let slot = allowed.clone();
    let (engine, events, _seen) = watched(&config(&game, env), move |app| {
        let policy = app.world().resource::<bevy_ts::net::NetPolicy>();
        let ok = |url: &str| policy.check(url, &[], false).is_ok();
        *slot.lock().unwrap() = Some((ok("wss://mac.tail.ts.net:8443/api/stream"), ok("ws://127.0.0.1:5000/")));
    });
    let seen = wait_event(&events, &["env {"]);
    let line: Value = serde_json::from_str(seen.last().unwrap()).unwrap();
    let shown: Value = serde_json::from_str(line["message"].as_str().unwrap().strip_prefix("env ").unwrap()).unwrap();
    assert_eq!(shown, json!({ "ABYME_URL": "https://mac.tail.ts.net:8443", "ABYME_VIEW": "town" }));
    assert_eq!(*allowed.lock().unwrap(), Some((true, false)));
    engine.destroy();
}

#[test]
fn a_destroyed_engine_can_be_made_again_from_the_start() {
    let _one = one_engine();
    let game = game("embed-again", HELLO, |_| {});
    for _ in 0..2 {
        let engine = create(&config(&game, json!({}))).unwrap();
        wait_event(&engine.events, &["hello from the game"]);
        let after = engine.destroy();
        assert!(after.last().is_some_and(|e| e.contains("exited")), "{after:?}");
    }
}

#[test]
fn the_abi_and_version_are_exported() {
    assert_eq!(bevy_ts_embed::ffi::bevy_ts_abi(), bevy_ts_embed::ffi::ABI);
    let v = unsafe { std::ffi::CStr::from_ptr(bevy_ts_embed::ffi::bevy_ts_version()) };
    assert_eq!(v.to_str().unwrap(), bevy_ts_embed::play::VERSION);
    let header = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/include/bevy_ts_embed.h")).unwrap();
    assert!(header.contains(&format!("#define BEVY_TS_ABI {}", bevy_ts_embed::ffi::ABI)), "the header's ABI is the library's");
}
