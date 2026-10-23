//! Change `viewport`, task 8.1: the demo walk against the real `bevy-ts dev --viewport` in a
//! process of its own, this test standing in for abyme: it looks the engine's Mach service up,
//! ticks it at 120 Hz as a display link would, reads the IOSurfaces it is given, sends input on
//! the channel, and plays the editor over HTTP (rewriting `game.tsx` when an edit is a literal).
//!
//! `cargo test -p bevy-ts-cli --test it viewport_walk:: -- --ignored --nocapture`
#![cfg(target_os = "macos")]

use crate::common;

use bevy::math::{Mat4, Quat, Vec2, Vec3, Vec4};
use bevy_ts_viewport::mach::{Incoming, Viewer, release_port};
use bevy_ts_viewport::surface::Surface;
use common::{project, write};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const ENEMY: &str = r##"import { Transform } from "bevy-ts/bevy";

export const Enemy = ({ at }: { at: [number, number, number] }) => (
  <entity name="enemy">
    <Transform translation={at} />
    <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
  </entity>
);
"##;

fn game(b_at: &str) -> String {
    format!(
        "import {{ app }} from \"bevy-ts\";\nimport {{ Enemy }} from \"./enemy.tsx\";\n\nconst spawn = (i: number): [number, number, number] => [i * -3, 0, 0];\n\napp.scene(\"arena\", () => (\n  <>\n    <Enemy key=\"b\" at={{{b_at}}} />\n    <Enemy key=\"c\" at={{spawn(2)}} />\n  </>\n));\n"
    )
}

const B_LINE: u32 = 8;
const W: f32 = 800.0;
const H: f32 = 600.0;

struct Engine(Child);

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn post(port: u16, method: &str, params: Value) -> Result<Value, String> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    let req = format!("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut out = String::new();
    s.read_to_string(&mut out).map_err(|e| e.to_string())?;
    let start = out.find("\r\n\r\n").ok_or("no body")? + 4;
    let body = &out[start..];
    // The body may be chunked: take the JSON object.
    let json_start = body.find('{').ok_or("no json")?;
    let json_end = body.rfind('}').ok_or("no json")? + 1;
    let v: Value = serde_json::from_str(&body[json_start..json_end]).map_err(|e| format!("{e}: {body}"))?;
    if let Some(e) = v.get("error") {
        return Err(format!("{method}: {e}"));
    }
    Ok(v["result"].clone())
}

/// The first message of a `+watch` stream (server-sent events), then the stream is dropped.
fn first_event(port: u16, method: &str) -> Value {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method }).to_string();
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let req = format!("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = vec![];
    let mut chunk = [0u8; 65536];
    loop {
        let n = s.read(&mut chunk).unwrap();
        assert!(n > 0, "{method}: stream ended");
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf);
        if let Some(i) = text.find("data: ")
            && let Some(j) = text[i..].find("\n\n")
        {
            let v: Value = serde_json::from_str(&text[i + 6..i + j]).unwrap();
            return v["result"].clone();
        }
    }
}

/// What the viewer has seen.
#[derive(Default)]
struct Seen {
    surfaces: HashMap<(u64, u64), Surface>,
    frames: Vec<Value>,
}

fn pump(viewer: &Viewer, seen: &mut Seen, wait: Duration) {
    while let Some(m) = viewer.recv(wait) {
        let Incoming::Message { method, params, port } = m else { continue };
        match method.as_str() {
            "surface" => {
                let port = port.unwrap();
                seen.surfaces.insert((params["ring"].as_u64().unwrap(), params["slot"].as_u64().unwrap()), Surface::from_mach_port(port).unwrap());
                release_port(port);
            }
            "frame" => seen.frames.push(params),
            _ => {}
        }
    }
}

/// Where a world point is in the hole, through the editor camera's pose as the protocol gives it.
fn project_point(cam: &Value, p: Vec3) -> Vec2 {
    let v = |a: &Value, n: usize| -> Vec<f32> { (0..n).map(|i| a[i].as_f64().unwrap() as f32).collect() };
    let t = v(&cam["translation"], 3);
    let r = v(&cam["rotation"], 4);
    let world = Mat4::from_rotation_translation(Quat::from_xyzw(r[0], r[1], r[2], r[3]), Vec3::new(t[0], t[1], t[2]));
    let fov = cam["fov"].as_f64().unwrap() as f32;
    // A perspective camera looks down -Z: x and y over depth, scaled by the field of view.
    let view = world.inverse() * Vec4::new(p.x, p.y, p.z, 1.0);
    let f = 1.0 / (fov / 2.0).tan();
    let ndc = Vec2::new(f / (W / H) * view.x / -view.z, f * view.y / -view.z);
    Vec2::new((ndc.x + 1.0) / 2.0 * W, (1.0 - ndc.y) / 2.0 * H)
}

fn input(viewer: &Viewer, e: Value) {
    viewer.send("input", e).unwrap();
}

#[test]
#[ignore = "spawns the engine binary and runs for several seconds; the viewport-walk process runs it"]
fn the_viewport_demo_walk() {
    let dir = project("viewport-walk", &[("enemy.tsx", ENEMY), ("game.tsx", &game("[3, 0, -4]"))]);
    let port = common::free_port();
    let child = Command::new(env!("CARGO_BIN_EXE_bevy-ts"))
        .args(["dev", dir.join("game.tsx").to_str().unwrap(), "--viewport", "--remote-port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _engine = Engine(child);
    let t0 = Instant::now();
    let hello = loop {
        if let Ok(h) = post(port, "bevy-ts/hello", Value::Null) {
            break h;
        }
        assert!(t0.elapsed() < Duration::from_secs(60), "the engine never answered");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(hello["viewport"], true);
    println!("1 started with no window: hello says viewport: true ({:.1} s)", t0.elapsed().as_secs_f64());

    // Attach, then tick at 120 Hz from a thread, as the hole's display link does.
    let a = post(port, "bevy-ts/viewport_attach", json!({ "w": W, "h": H, "scale": 2, "hz": 120 })).unwrap();
    let viewer = Arc::new(Viewer::connect(a["service"].as_str().unwrap()).unwrap());
    let ticking = Arc::new(AtomicBool::new(true));
    let ticker = {
        let (viewer, ticking) = (viewer.clone(), ticking.clone());
        std::thread::spawn(move || {
            let period = Duration::from_nanos(8_333_333);
            let mut next = Instant::now();
            while ticking.load(Ordering::Relaxed) {
                let ts = bevy_ts_viewport::shared::now_ns();
                let _ = viewer.send("tick", json!({ "ts": ts, "targetTs": ts + 8_333_333 }));
                next += period;
                std::thread::sleep(next.saturating_duration_since(Instant::now()));
            }
        })
    };
    input(&viewer, json!({ "type": "focus", "on": true }));
    let mut seen = Seen::default();
    let t0 = Instant::now();
    while seen.frames.len() < 10 {
        pump(&viewer, &mut seen, Duration::from_millis(20));
        assert!(t0.elapsed() < Duration::from_secs(60), "no frames");
    }
    // A second of play at the viewer's pace.
    let before = post(port, "bevy-ts/viewport_stats", Value::Null).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let after = post(port, "bevy-ts/viewport_stats", Value::Null).unwrap();
    let fps = after["frames"].as_u64().unwrap() - before["frames"].as_u64().unwrap();
    pump(&viewer, &mut seen, Duration::from_millis(20));
    println!(
        "2 play: {} surfaces of {}x{}, {fps} frames in 1 s at 120 Hz ticks, {} skipped, {} dropped ticks, gpu {:.2} ms",
        seen.surfaces.len(),
        seen.surfaces.values().next().unwrap().width(),
        seen.surfaces.values().next().unwrap().height(),
        after["skipped"],
        after["droppedTicks"],
        after["gpuMs"].as_f64().unwrap_or(0.0)
    );

    // Input to frame through the runner in play: 40 events at spread phases of the tick.
    let base = post(port, "bevy-ts/viewport_stats", Value::Null).unwrap()["inputToFrame"]["samples"].as_u64().unwrap();
    for i in 0..40u64 {
        input(&viewer, json!({ "type": "pointer", "kind": "move", "x": 10 + i, "y": 10 }));
        std::thread::sleep(Duration::from_micros(37_000 + (i * 1777) % 8333));
    }
    let lat = post(port, "bevy-ts/viewport_stats", Value::Null).unwrap()["inputToFrame"].clone();
    println!("2b play input to frame over {} events: median {:.1} ms, p95 {:.1} ms", lat["samples"].as_u64().unwrap() - base, lat["medianMs"].as_f64().unwrap(), lat["p95Ms"].as_f64().unwrap());

    // Edit mode, then select b by its line, as the outliner would.
    let m = post(port, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    assert_eq!(m["paused"], true);
    std::thread::sleep(Duration::from_millis(100));
    let sel = post(port, "bevy-ts/viewport_select", json!({ "file": "game.tsx", "line": B_LINE })).unwrap();
    let b = sel["items"][0]["entity"].as_u64().unwrap();
    assert_eq!(sel["origin"], "editor");
    println!("3 edit: paused; selected b by game.tsx:{B_LINE} -> entity {b}, route {}", sel["items"][0]["routes"]["translation"]["kind"]);

    // Select it again by a click on it.
    let cam = post(port, "bevy-ts/viewport_camera", Value::Null).unwrap();
    let b_at = project_point(&cam, Vec3::new(3.0, 0.0, -4.0));
    let pick = post(port, "bevy-ts/viewport_pick", json!({ "x": b_at.x, "y": b_at.y })).unwrap();
    assert_eq!(pick["hits"][0]["selects"].as_u64(), Some(b), "{pick}");
    post(port, "bevy-ts/viewport_select", json!({})).unwrap();
    let click = |x: f32, y: f32| {
        input(&viewer, json!({ "type": "pointer", "kind": "move", "x": x, "y": y }));
        std::thread::sleep(Duration::from_millis(60));
        input(&viewer, json!({ "type": "pointer", "kind": "down", "x": x, "y": y, "button": "left", "clicks": 1 }));
        std::thread::sleep(Duration::from_millis(40));
        input(&viewer, json!({ "type": "pointer", "kind": "up", "x": x, "y": y, "button": "left", "clicks": 1 }));
        std::thread::sleep(Duration::from_millis(60));
    };
    click(b_at.x, b_at.y);
    let sel = first_event(port, "bevy-ts/viewport_selection+watch");
    assert_eq!(sel["items"][0]["entity"].as_u64(), Some(b), "{sel}");
    assert_eq!(sel["origin"], "viewport");
    println!("4 clicked b at ({:.0}, {:.0}): selected, origin viewport", b_at.x, b_at.y);

    // Drag b's X handle 80 points to the right.
    let cam_pos = Vec3::new(cam["translation"][0].as_f64().unwrap() as f32, cam["translation"][1].as_f64().unwrap() as f32, cam["translation"][2].as_f64().unwrap() as f32);
    let b_pos = Vec3::new(3.0, 0.0, -4.0);
    let handle = project_point(&cam, b_pos + Vec3::X * 0.6 * (cam_pos - b_pos).length() * 0.1);
    input(&viewer, json!({ "type": "pointer", "kind": "move", "x": handle.x, "y": handle.y }));
    std::thread::sleep(Duration::from_millis(60));
    input(&viewer, json!({ "type": "pointer", "kind": "down", "x": handle.x, "y": handle.y, "button": "left" }));
    std::thread::sleep(Duration::from_millis(40));
    for i in 1..=8 {
        input(&viewer, json!({ "type": "pointer", "kind": "move", "x": handle.x + 10.0 * i as f32, "y": handle.y }));
        std::thread::sleep(Duration::from_millis(17));
    }
    input(&viewer, json!({ "type": "pointer", "kind": "up", "x": handle.x + 80.0, "y": handle.y, "button": "left" }));
    std::thread::sleep(Duration::from_millis(100));
    let edits = first_event(port, "bevy-ts/viewport_edits+watch");
    let edit = edits["edits"].as_array().unwrap().last().cloned().expect("an edit");
    let new_x = edit["after"]["translation"][0].as_f64().unwrap();
    assert_eq!(edit["route"]["kind"], "tsx");
    assert!(new_x > 3.2, "{edit}");
    println!("5 dragged b: x 3 -> {new_x:.3}, one edit, route tsx at {}:{}", edit["route"]["source"].as_array().unwrap().last().unwrap()["file"], edit["route"]["source"].as_array().unwrap().last().unwrap()["line"]);

    // The editor finds a literal at the call site, rewrites it and answers.
    let t_save = Instant::now();
    write(&dir, "game.tsx", &game(&format!("[{:.3}, 0, -4]", new_x)));
    let t0 = Instant::now();
    loop {
        let r = post(port, "bevy-ts/hello", Value::Null).unwrap();
        if r["generation"].as_u64().unwrap() >= 2 {
            break;
        }
        assert!(t0.elapsed() < Duration::from_secs(10), "no reload");
        std::thread::sleep(Duration::from_millis(10));
    }
    let reload_ms = t_save.elapsed().as_secs_f64() * 1000.0;
    let r = post(port, "bevy-ts/viewport_edit_result", json!({ "id": edit["id"], "status": "written" })).unwrap();
    assert_eq!(r["status"], "written");
    let sel = first_event(port, "bevy-ts/viewport_selection+watch");
    let b2 = sel["items"][0]["entity"].as_u64().unwrap();
    let comps = post(port, "bevy-ts/get_components", json!({ "entity": b2, "components": ["bevy_transform::components::transform::Transform"] })).unwrap();
    let x = comps["components"]["bevy_transform::components::transform::Transform"]["translation"][0].as_f64().unwrap();
    assert!((x - new_x).abs() < 1e-3, "no jump: {x} vs {new_x}");
    println!("6 editor wrote the literal: reload in {reload_ms:.0} ms, b at x {x:.3} (no jump), still selected");

    // Step one frame in edit mode.
    pump(&viewer, &mut seen, Duration::from_millis(20));
    let n = seen.frames.len();
    let s = post(port, "bevy-ts/step", json!({ "frames": 1 })).unwrap();
    assert_eq!(s["paused"], true);
    std::thread::sleep(Duration::from_millis(100));
    pump(&viewer, &mut seen, Duration::from_millis(20));
    println!("7 stepped one frame in edit: {} frames since", seen.frames.len() - n);

    // Resize the hole.
    viewer.send("resize", json!({ "w": 900, "h": 500, "scale": 2 })).unwrap();
    let t0 = Instant::now();
    loop {
        pump(&viewer, &mut seen, Duration::from_millis(20));
        if seen.frames.last().is_some_and(|f| f["w"] == 1800 && f["h"] == 1000) {
            break;
        }
        assert!(t0.elapsed() < Duration::from_secs(5), "no frame at the new size");
    }
    println!("8 resized to 900x500 points: frames at 1800x1000 in {:.0} ms", t0.elapsed().as_secs_f64() * 1000.0);

    let stats = post(port, "bevy-ts/viewport_stats", Value::Null).unwrap();
    println!("stats: {stats}");
    ticking.store(false, Ordering::Relaxed);
    ticker.join().unwrap();
}
