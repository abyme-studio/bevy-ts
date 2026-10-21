//! The arena under `bevy-ts dev --viewport` in a process of its own: its meshes are lit in play,
//! and lit again after edit mode and back. A mesh with no light renders black.
//!
//! `cargo test -p bevy-ts-cli --test play_after_edit -- --ignored --nocapture`
#![cfg(target_os = "macos")]

use bevy_ts_viewport::mach::{Incoming, Viewer, release_port};
use bevy_ts_viewport::surface::Surface;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const W: u32 = 400;
const H: u32 = 300;

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
    let body = &out[out.find("\r\n\r\n").ok_or("no body")? + 4..];
    let v: Value = serde_json::from_str(&body[body.find('{').ok_or("no json")?..body.rfind('}').ok_or("no json")? + 1]).map_err(|e| e.to_string())?;
    match v.get("error") {
        Some(e) => Err(format!("{method}: {e}")),
        None => Ok(v["result"].clone()),
    }
}

#[derive(Default)]
struct Seen {
    surfaces: HashMap<(u64, u64), Surface>,
    frames: Vec<Value>,
}

/// `n` more frames, then the last frame's pixels on a grid.
fn frames(viewer: &Viewer, seen: &mut Seen, n: usize) -> Vec<[u8; 4]> {
    let want = seen.frames.len() + n;
    let t0 = Instant::now();
    while seen.frames.len() < want {
        assert!(t0.elapsed() < Duration::from_secs(60), "no frames: {} of {want}", seen.frames.len());
        if let Some(m) = viewer.recv(Duration::from_millis(20)) {
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
    let f = seen.frames.last().unwrap();
    let grid: Vec<(u32, u32)> = (0..H).step_by(2).flat_map(|y| (0..W).step_by(2).map(move |x| (x, y))).collect();
    seen.surfaces[&(f["ring"].as_u64().unwrap(), f["slot"].as_u64().unwrap())].read(&grid)
}

/// Pixels with no light on them: every channel near zero. The clear colour and the HUD are not.
fn black(px: &[[u8; 4]]) -> usize {
    px.iter().filter(|p| p[..3].iter().all(|c| *c < 6)).count()
}

#[test]
#[ignore = "spawns the engine binary on the arena; the play-after-edit process runs it"]
fn the_arena_is_lit_after_edit_and_back_to_play() {
    let arena = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena/src/game.tsx");
    let log = Path::new(env!("CARGO_TARGET_TMPDIR")).join("play-after-edit-engine.log");
    let port = 16050 + (std::process::id() % 40) as u16;
    let child = Command::new(env!("CARGO_BIN_EXE_bevy-ts"))
        .args(["dev", arena.to_str().unwrap(), "--viewport", "--remote-port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log).unwrap())
        .spawn()
        .unwrap();
    let _engine = Engine(child);
    let t0 = Instant::now();
    while post(port, "bevy-ts/hello", Value::Null).is_err() {
        assert!(t0.elapsed() < Duration::from_secs(60), "the engine never answered");
        std::thread::sleep(Duration::from_millis(50));
    }
    let a = post(port, "bevy-ts/viewport_attach", json!({ "w": W, "h": H, "scale": 1, "hz": 120 })).unwrap();
    let viewer = Arc::new(Viewer::connect(a["service"].as_str().unwrap()).unwrap());
    // Ticks at 120 Hz from a thread, as the hole's display link does.
    let ticking = Arc::new(AtomicBool::new(true));
    let ticker = {
        let (viewer, ticking) = (viewer.clone(), ticking.clone());
        std::thread::spawn(move || {
            while ticking.load(Ordering::Relaxed) {
                let ts = bevy_ts_viewport::shared::now_ns();
                let _ = viewer.send("tick", json!({ "ts": ts, "targetTs": ts + 8_333_333 }));
                std::thread::sleep(Duration::from_nanos(8_333_333));
            }
        })
    };
    viewer.send("input", json!({ "type": "focus", "on": true })).unwrap();
    let mut seen = Seen::default();
    let play = black(&frames(&viewer, &mut seen, 60));
    post(port, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    let edit = black(&frames(&viewer, &mut seen, 30));
    post(port, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    let again = black(&frames(&viewer, &mut seen, 30));
    ticking.store(false, Ordering::Relaxed);
    ticker.join().unwrap();
    println!("black pixels: play {play}, edit {edit}, play again {again}");
    let log = std::fs::read_to_string(&log).unwrap();
    assert!(!log.contains("panicked"), "the engine panicked:\n{log}");
    assert!(again <= play + 20, "meshes went black after edit and back to play: {play} black pixels before, {again} after");
}
