//! The arena under `bevy-ts dev --viewport` in a process of its own: its meshes are lit in play,
//! and lit again after edit mode and back. A mesh with no light renders black. Booted in edit, as
//! the studio boots it, Play draws the game camera's picture into the viewport.
//!
//! `cargo test -p bevy-ts-cli --test it play_after_edit:: -- --ignored --nocapture`
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

/// The engine on the arena, attached at W×H, ticking at 120 Hz from a thread as the hole's display
/// link does; `edit` boots it as the studio does (`--edit`).
struct Run {
    port: u16,
    viewer: Arc<Viewer>,
    ticking: Arc<AtomicBool>,
    ticker: Option<std::thread::JoinHandle<()>>,
    log: std::path::PathBuf,
    _engine: Engine,
}

impl Run {
    fn start(edit: bool, name: &str) -> Run {
        let arena = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena/src/game.tsx");
        let log = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-engine.log"));
        let port = crate::common::free_port();
        let mut args = vec!["dev".to_string(), arena.to_str().unwrap().to_string(), "--viewport".into()];
        if edit {
            args.push("--edit".into());
        }
        args.extend(["--remote-port".to_string(), port.to_string()]);
        let child = Command::new(env!("CARGO_BIN_EXE_bevy-ts")).args(&args).stdout(Stdio::null()).stderr(std::fs::File::create(&log).unwrap()).spawn().unwrap();
        let engine = Engine(child);
        let t0 = Instant::now();
        while post(port, "bevy-ts/hello", Value::Null).is_err() {
            assert!(t0.elapsed() < Duration::from_secs(60), "the engine never answered");
            std::thread::sleep(Duration::from_millis(50));
        }
        let a = post(port, "bevy-ts/viewport_attach", json!({ "w": W, "h": H, "scale": 1, "hz": 120 })).unwrap();
        let viewer = Arc::new(Viewer::connect(a["service"].as_str().unwrap()).unwrap());
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
        Run { port, viewer, ticking, ticker: Some(ticker), log, _engine: engine }
    }

    /// Stops ticking; the engine's log, which must hold no panic.
    fn finish(&mut self) -> String {
        self.ticking.store(false, Ordering::Relaxed);
        if let Some(t) = self.ticker.take() {
            t.join().unwrap();
        }
        let log = std::fs::read_to_string(&self.log).unwrap();
        assert!(!log.contains("panicked"), "the engine panicked:\n{log}");
        log
    }
}

/// Writes a frame's grid (W/2 by H/2) as a PPM beside the engine log, to look at when it fails.
fn dump(px: &[[u8; 4]], name: &str) {
    let mut out = format!("P6 {} {} 255\n", W / 2, H / 2).into_bytes();
    // The surface is BGRA.
    out.extend(px.iter().flat_map(|p| [p[2], p[1], p[0]]));
    std::fs::write(Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.ppm")), out).unwrap();
}

/// The share of pixels that are the frame's most common colour: near 1 for a frame only cleared.
fn plain(px: &[[u8; 4]]) -> f32 {
    let mut counts: HashMap<[u8; 3], usize> = HashMap::new();
    for p in px {
        *counts.entry([p[0], p[1], p[2]]).or_default() += 1;
    }
    *counts.values().max().unwrap_or(&0) as f32 / px.len().max(1) as f32
}

#[test]
#[ignore = "spawns the engine binary on the arena; the play-after-edit process runs it"]
fn the_arena_is_lit_after_edit_and_back_to_play() {
    let mut run = Run::start(false, "play-after-edit");
    let mut seen = Seen::default();
    let play = black(&frames(&run.viewer, &mut seen, 60));
    post(run.port, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    let edit = black(&frames(&run.viewer, &mut seen, 30));
    post(run.port, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    let again = frames(&run.viewer, &mut seen, 30);
    run.finish();
    dump(&again, "play-after-edit");
    println!("plain share: play boot, play again {:.3}", plain(&again));
    let again = black(&again);
    println!("black pixels: play {play}, edit {edit}, play again {again}");
    assert!(again <= play + 20, "meshes went black after edit and back to play: {play} black pixels before, {again} after");
}

/// The studio boots the engine in edit mode and presses Play: the game camera's picture reaches the
/// viewport, lit and drawn, not a frame only cleared.
#[test]
#[ignore = "spawns the engine binary on the arena; the play-after-edit process runs it"]
fn play_from_an_edit_boot_draws_the_arena_into_the_viewport() {
    let mut run = Run::start(true, "play-from-edit");
    let mut seen = Seen::default();
    let edit = frames(&run.viewer, &mut seen, 30);
    let r = post(run.port, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    assert_eq!(r["mode"], "play");
    let play = frames(&run.viewer, &mut seen, 60);
    post(run.port, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    frames(&run.viewer, &mut seen, 30);
    post(run.port, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    let again = frames(&run.viewer, &mut seen, 60);
    run.finish();
    dump(&edit, "play-from-edit-edit");
    dump(&play, "play-from-edit-play");
    dump(&again, "play-from-edit-again");
    let n = play.len();
    println!(
        "plain share: edit {:.3}, play {:.3}, play again {:.3}; black: play {} of {n}, play again {}",
        plain(&edit),
        plain(&play),
        plain(&again),
        black(&play),
        black(&again)
    );
    for (name, px) in [("play", &play), ("play after a stop", &again)] {
        // The arena's own play frame is about 0.88 sky; a frame only cleared is 1.
        assert!(plain(px) < 0.95,"{name}: the viewport is one plain colour ({:.3} of it)", plain(px));
        assert!(black(px) < n / 2, "{name}: the viewport is unlit ({} of {n} pixels black)", black(px));
    }
}
