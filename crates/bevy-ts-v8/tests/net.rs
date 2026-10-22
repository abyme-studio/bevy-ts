//! Change script-networking, end to end in V8 and headless: scripts' `WebSocket`s and timers
//! through the fake transport, with `Time<Real>` advanced 16 ms a frame.

mod common;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy_ts::generation::{self, ReloadStatus};
use bevy_ts::net::{self, ConnId, FakeTransport, Frame, NetPolicy};
use bevy_ts::runtime::{ScriptClock, ScriptRuntime};
use bevy_ts::EngineMode;
use common::*;
use std::time::Duration;

fn net_app() -> (App, FakeTransport) {
    let mut app = app();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(16)));
    let allow = vec!["127.0.0.1:4477".to_string(), "${ABYME_URL}".to_string()];
    let env = vec!["ABYME_URL".to_string(), "ABYME_KEY".to_string()];
    let vars = |n: &str| match n {
        "ABYME_URL" => Some("http://127.0.0.1:5000".to_string()),
        "HOME" => Some("/home/me".to_string()),
        _ => None,
    };
    app.insert_resource(NetPolicy::from_config(&allow, &env, &vars).0);
    let fake = net::install_fake(&mut app);
    // A first frame, so real time has started.
    app.update();
    (app, fake)
}

fn calls(app: &App) -> u64 {
    app.world().non_send::<ScriptRuntime>().backend.calls_into_js()
}

fn has(app: &App, needle: &str) -> bool {
    messages(app).iter().any(|m| m.contains(needle))
}

/// The client's opening: connect, say hello back, log what arrives.
const CLIENT: &str = r#"
import { app } from "bevy-ts";
app.entry("test/count", () => {
  let n = 0;
  ws.onmessage = (e) => { n++; if (e.data === "499") console.log(`last ${e.data} count ${n}`); };
});
app.entry("test/backoff", () => { setTimeout(() => console.log("backoff"), 1000); });
const ws = new WebSocket(new URL("/api/stream", "http://127.0.0.1:4477").toString().replace("http:", "ws:"));
ws.onopen = () => { console.log("open"); ws.send("hello-back"); };
ws.onmessage = (e) => console.log("message " + (typeof e.data === "string" ? e.data : "bytes " + e.data.byteLength));
ws.onclose = (e) => console.log(`close ${e.code} ${e.wasClean}`);
ws.onerror = () => console.log("error");
globalThis.ws = ws;
"#;

/// Loads the client and accepts its connection; returns the wire id.
fn connected(app: &mut App, fake: &FakeTransport) -> ConnId {
    load(app, &[("client.js", CLIENT)]);
    let (wire, conn) = fake.last().expect("the module's open reached the transport");
    assert_eq!(conn.request.url, "ws://127.0.0.1:4477/api/stream");
    fake.accept(wire, "");
    app.update();
    assert!(has(app, "open"), "{:?}", messages(app));
    wire
}

#[test]
fn the_abyme_client_handshake_runs_headless() {
    let (mut app, fake) = net_app();
    let wire = connected(&mut app, &fake);
    fake.push_text(wire, r#"{"hello":1}"#);
    fake.push_binary(wire, &[1, 2, 3]);
    app.update();
    assert!(has(&app, r#"message {"hello":1}"#), "{:?}", messages(&app));
    assert!(has(&app, "message bytes 3"), "{:?}", messages(&app));
    assert_eq!(fake.conn(wire).unwrap().sent, vec![Frame::Text("hello-back".into())]);
}

#[test]
fn process_env_shows_only_what_env_names() {
    let (mut app, _fake) = net_app();
    load(&mut app, &[("env.js", r#"console.log(`env ${process.env.ABYME_URL} ${process.env.ABYME_KEY} ${process.env.HOME} ${Object.isFrozen(process.env)}`);"#)]);
    app.update();
    assert!(has(&app, "env http://127.0.0.1:5000 undefined undefined true"), "{:?}", messages(&app));
}

#[test]
fn many_messages_one_call_and_idle_frames_none() {
    let (mut app, fake) = net_app();
    let wire = connected(&mut app, &fake);
    updates(&mut app, 2);
    let before = calls(&app);
    for _ in 0..100 {
        app.update();
    }
    assert_eq!(calls(&app), before, "no call into JS while idle");
    entry(&mut app, "test/count");
    let before = calls(&app);
    for i in 0..500 {
        fake.push_text(wire, &format!("{i}"));
    }
    app.update();
    assert_eq!(calls(&app), before + 1, "500 messages in one call");
    assert!(has(&app, "last 499 count 500"), "{:?}", messages(&app));
}

/// Calls an entry of the live generation, as a system would run its code.
fn entry(app: &mut App, name: &str) {
    bevy_ts::plugin_api::call_entry(app.world_mut(), name, b"").unwrap();
}

#[test]
fn set_timeout_50_fires_in_the_fourth_frame() {
    let (mut app, _fake) = net_app();
    load(
        &mut app,
        &[("timer.js", r#"import { app } from "bevy-ts"; app.entry("test/arm", () => { setTimeout((a, b) => console.log(`fired ${a}${b}`), 50, "o", "k"); });"#)],
    );
    app.update();
    entry(&mut app, "test/arm");
    for frame in 1..=4 {
        app.update();
        assert_eq!(has(&app, "fired ok"), frame == 4, "frame {frame}: {:?}", messages(&app));
    }
}

#[test]
fn a_paused_game_still_fires_timers_and_delivers() {
    let (mut app, fake) = net_app();
    let wire = connected(&mut app, &fake);
    app.world_mut().resource_mut::<ScriptClock>().paused = true;
    entry(&mut app, "test/backoff");
    fake.push_text(wire, "while paused");
    updates(&mut app, 70);
    assert!(has(&app, "message while paused"), "{:?}", messages(&app));
    assert!(has(&app, "backoff"), "{:?}", messages(&app));
}

#[test]
fn saving_closes_the_old_generations_socket_quietly() {
    let (mut app, fake) = net_app();
    let old = connected(&mut app, &fake);
    load(&mut app, &[("client.js", CLIENT)]);
    assert_eq!(fake.conn(old).unwrap().closed, Some((Some(1001), "going away".into())));
    let (new, _) = fake.last().unwrap();
    assert_ne!(new, old);
    updates(&mut app, 3);
    assert!(!has(&app, "close"), "generation 1's handlers hear nothing: {:?}", messages(&app));
}

#[test]
fn a_failed_reload_keeps_the_live_socket() {
    let (mut app, fake) = net_app();
    let wire = connected(&mut app, &fake);
    let bad = format!("{CLIENT}\nthrow new Error('boom');");
    let report = generation::apply(app.world_mut(), set(&[("client.js", &bad)]), false);
    assert_eq!(report.status, ReloadStatus::Failed);
    assert_eq!(fake.conns().len(), 1, "the failed load opened nothing");
    fake.push_text(wire, "after");
    app.update();
    assert!(has(&app, "message after"), "{:?}", messages(&app));
    assert!(fake.conn(wire).unwrap().closed.is_none());
}

#[test]
fn a_restart_closes_every_socket() {
    let (mut app, fake) = net_app();
    let wire = connected(&mut app, &fake);
    let report = generation::apply(app.world_mut(), set(&[("other.js", "")]), true);
    assert_eq!(report.status, ReloadStatus::Applied);
    assert_eq!(fake.conn(wire).unwrap().closed, Some((Some(1001), "going away".into())));
}

fn set_mode(app: &mut App, mode: EngineMode) {
    app.world_mut().resource_mut::<NextState<EngineMode>>().set(mode);
    app.update();
}

#[test]
fn back_to_edit_closes_and_play_delivers_1001() {
    let (mut app, fake) = net_app();
    let wire = connected(&mut app, &fake);
    set_mode(&mut app, EngineMode::Edit);
    assert_eq!(fake.conn(wire).unwrap().closed, Some((Some(1001), "going away".into())));
    updates(&mut app, 5);
    assert!(!has(&app, "close"), "nothing is delivered in edit mode");
    set_mode(&mut app, EngineMode::Play);
    assert!(has(&app, "close 1001 true"), "{:?}", messages(&app));
}

#[test]
fn a_timer_set_before_edit_fires_at_the_first_frame_of_play() {
    let (mut app, _fake) = net_app();
    load(&mut app, &[("t.js", r#"import { app } from "bevy-ts"; app.entry("test/arm", () => { setTimeout(() => console.log("fired"), 100); });"#)]);
    entry(&mut app, "test/arm");
    set_mode(&mut app, EngineMode::Edit);
    updates(&mut app, 60);
    assert!(!has(&app, "fired"));
    set_mode(&mut app, EngineMode::Play);
    assert!(has(&app, "fired"), "{:?}", messages(&app));
}

#[test]
fn an_edit_mode_load_opens_nothing() {
    let (mut app, fake) = net_app();
    set_mode(&mut app, EngineMode::Edit);
    load(&mut app, &[("client.js", CLIENT)]);
    app.update();
    assert!(fake.conns().is_empty());
    assert!(has(&app, "networking runs only in play"), "{:?}", messages(&app));
    set_mode(&mut app, EngineMode::Play);
    assert!(has(&app, "close 1006 false"), "{:?}", messages(&app));
}

#[test]
fn a_disallowed_host_is_refused_with_the_webs_events() {
    let (mut app, fake) = net_app();
    load(&mut app, &[("bad.js", "const ws = new WebSocket('ws://example.com/'); ws.onerror = () => console.log('error'); ws.onclose = (e) => console.log(`close ${e.code} ${ws.readyState}`);")]);
    updates(&mut app, 2);
    assert!(fake.conns().is_empty());
    let m = messages(&app);
    let e = m.iter().position(|l| l == "error").expect("error fired");
    assert!(m[e + 1..].iter().any(|l| l == "close 1006 3"), "{m:?}");
    assert!(has(&app, "example.com:80 is not in net.allow"), "{m:?}");
}

#[test]
fn a_dropped_connection_reports_1006() {
    let (mut app, fake) = net_app();
    let wire = connected(&mut app, &fake);
    fake.drop_conn(wire);
    app.update();
    let m = messages(&app);
    let e = m.iter().position(|l| l == "error").expect("error fired");
    assert!(m[e + 1..].iter().any(|l| l == "close 1006 false"), "{m:?}");
}

/// Task 5.4: a burst of 2,000 1 KiB messages in one frame, each parsed by the script. Writes the
/// frame's main-thread cost to bench/networking/results/burst.md. Run in release:
/// `cargo test --release -p bevy-ts-v8 --test net burst_bench -- --ignored --nocapture`.
#[test]
#[ignore]
fn burst_bench() {
    let (mut app, fake) = net_app();
    let client = r#"
const ws = new WebSocket("ws://127.0.0.1:4477/api/stream");
let n = 0, sum = 0;
ws.onmessage = (e) => { const r = JSON.parse(e.data); n++; sum += r.seq; };
"#;
    load(&mut app, &[("bench.js", client)]);
    let (wire, _) = fake.last().unwrap();
    fake.accept(wire, "");
    app.update();
    // A 1 KiB record, as abymed's stream sends.
    let pad = "x".repeat(1024 - 40);
    let record = |i: usize| format!(r#"{{"seq":{i:>8},"kind":"rec","pad":"{pad}"}}"#);
    assert!((1000..=1100).contains(&record(0).len()), "{}", record(0).len());
    let frame = |app: &mut App| {
        let t = std::time::Instant::now();
        app.update();
        t.elapsed().as_secs_f64() * 1000.0
    };
    let mut idle = Vec::new();
    let mut burst = Vec::new();
    for _ in 0..8 {
        // Warm and settle.
        for _ in 0..5 {
            app.update();
        }
        idle.push(frame(&mut app));
        for i in 0..2000 {
            fake.push_text(wire, &record(i));
        }
        burst.push(frame(&mut app));
    }
    let med = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let (mi, mb) = (med(&mut idle.clone()), med(&mut burst.clone()));
    let out = format!(
        "# Burst of 2,000 1 KiB messages in one frame\n\nHeadless app (MinimalPlugins), V8 backend, fake transport; the script JSON-parses every message.\n`cargo test --release -p bevy-ts-v8 --test net burst_bench -- --ignored --nocapture`, 8 rounds.\n\n| frame | median ms | all ms |\n| --- | --- | --- |\n| idle | {mi:.3} | {idle:.3?} |\n| 2,000 × 1 KiB | {mb:.3} | {burst:.3?} |\n\nThe burst's main-thread cost over an idle frame: {:.3} ms.\n",
        mb - mi
    );
    println!("{out}");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/networking/results");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("burst.md"), out).unwrap();
}
