//! Change `asset-preview`, task 4.2: what an engine preview costs the game. The arena runs in the
//! real `bevy-ts dev --viewport` (release), its game viewport ticked at 120 Hz as abyme's display
//! link does, in one scenario per run:
//!
//! - `none`: the game alone;
//! - `visible`: an engine preview of the model attached beside it, ticked and visible;
//! - `hidden`: that preview attached but hidden;
//! - `threejs`: the game alone, while a three.js viewer of the model runs elsewhere (started by
//!   whoever runs this);
//! - `idle`: no viewport visible, the engine's idle cost in play;
//! - `idle-edit`: the same in edit mode, as the studio keeps an open project's engine.
//!
//! `PREVIEW_MODEL=/path/model.glb SCENARIO=visible cargo test --release -p bevy-ts-cli --test it preview_bench:: -- --ignored --nocapture`
//! Each run appends a row to `target/preview-bench.md`.
#![cfg(target_os = "macos")]

use crate::common;

use common::{BOUND, Engine, LAUNCH};
use bevy_ts_viewport::mach::Viewer;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn post(port: u16, method: &str, params: Value) -> Result<Value, String> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    let req = format!("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut out = String::new();
    s.read_to_string(&mut out).map_err(|e| e.to_string())?;
    let body = &out[out.find("\r\n\r\n").ok_or("no body")? + 4..];
    let (a, b) = (body.find('{').ok_or("no json")?, body.rfind('}').ok_or("no json")? + 1);
    let v: Value = serde_json::from_str(&body[a..b]).map_err(|e| format!("{e}: {body}"))?;
    if let Some(e) = v.get("error") {
        return Err(format!("{method}: {e}"));
    }
    Ok(v["result"].clone())
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

/// A viewer that says hello, ticks at 120 Hz and drains what it is sent, until dropped.
struct Ticked {
    viewer: Arc<Viewer>,
    on: Arc<AtomicBool>,
}

impl Ticked {
    fn new(service: &str) -> Ticked {
        let viewer = Arc::new(Viewer::connect(service).unwrap());
        let on = Arc::new(AtomicBool::new(true));
        let (v, o) = (viewer.clone(), on.clone());
        std::thread::spawn(move || {
            let period = Duration::from_nanos(8_333_333);
            let mut next = Instant::now();
            while o.load(Ordering::Relaxed) {
                let ts = bevy_ts_viewport::shared::now_ns();
                let _ = v.send("tick", json!({ "ts": ts, "targetTs": ts + 8_333_333 }));
                // Surfaces come with a port; a viewer that keeps it holds the slot in use.
                while let Some(m) = v.recv(Duration::ZERO) {
                    if let bevy_ts_viewport::mach::Incoming::Message { port: Some(p), .. } = m {
                        bevy_ts_viewport::mach::release_port(p);
                    }
                }
                next += period;
                std::thread::sleep(next.saturating_duration_since(Instant::now()));
            }
        });
        Ticked { viewer, on }
    }
}

impl Drop for Ticked {
    fn drop(&mut self) {
        self.on.store(false, Ordering::Relaxed);
    }
}

/// `ps` of one process: (RSS in KiB, CPU seconds).
fn ps(pid: u32) -> (u64, f64) {
    let out = Command::new("ps").args(["-o", "rss=,time=", "-p", &pid.to_string()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    let mut it = s.split_whitespace();
    let rss = it.next().unwrap().parse().unwrap();
    // [[dd-]hh:]mm:ss.cc
    let secs = it.next().unwrap().split(':').rev().enumerate().map(|(i, p)| p.parse::<f64>().unwrap() * 60f64.powi(i as i32)).sum();
    (rss, secs)
}

/// The process's physical footprint in MB, which counts its Metal allocations (`footprint`).
fn footprint(pid: u32) -> f64 {
    let out = Command::new("footprint").arg(pid.to_string()).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().find(|l| l.contains("Footprint:")).unwrap_or("");
    let mut words = line.split_whitespace().skip_while(|w| *w != "Footprint:").skip(1);
    let (n, unit) = (words.next().and_then(|n| n.parse::<f64>().ok()).unwrap_or(0.0), words.next().unwrap_or("MB"));
    if unit == "GB" { n * 1024.0 } else if unit == "KB" { n / 1024.0 } else { n }
}

fn ms(v: &Value) -> String {
    v.as_f64().map_or("-".into(), |x| format!("{x:.2}"))
}

#[test]
#[ignore = "a measurement: runs the release engine for several seconds; see the header"]
fn preview_cost() {
    let scenario = std::env::var("SCENARIO").unwrap_or_else(|_| "none".into());
    let secs: u64 = std::env::var("SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
    let model = std::env::var("PREVIEW_MODEL").expect("PREVIEW_MODEL: a .glb to preview");
    let dir = std::env::temp_dir().join("bevy-ts-preview-bench");
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena"), &dir);
    std::fs::create_dir_all(dir.join("assets/models")).unwrap();
    std::fs::copy(&model, dir.join("assets/models/model.glb")).unwrap();

    let port = common::free_port();
    let engine = Engine::spawn(
        Command::new(env!("CARGO_BIN_EXE_bevy-ts"))
            .args(["dev", dir.join("src/game.tsx").to_str().unwrap(), "--viewport", "--remote-port", &port.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    );
    let pid = engine.id();
    let t0 = Instant::now();
    while post(port, "bevy-ts/hello", Value::Null).map(|h| h["generation"].as_u64().unwrap_or(0)).unwrap_or(0) == 0 {
        assert!(t0.elapsed() < LAUNCH, "the arena never loaded");
        std::thread::sleep(Duration::from_millis(100));
    }
    let game = post(port, "bevy-ts/viewport_attach", json!({ "w": 1280, "h": 720, "scale": 2, "hz": 120 })).unwrap();
    let game = Ticked::new(game["service"].as_str().unwrap());
    let mut preview_viewport = None;
    let mut _preview = None;
    if scenario == "visible" || scenario == "hidden" {
        let t0 = Instant::now();
        let id = loop {
            let r = post(port, "bevy-ts/asset_preview", json!({ "path": "models/model.glb" })).unwrap();
            if let Some(id) = r["preview"].as_u64() {
                break id;
            }
            assert!(t0.elapsed() < BOUND, "the model never loaded");
            std::thread::sleep(Duration::from_millis(50));
        };
        let a = post(port, "bevy-ts/viewport_attach", json!({ "w": 800, "h": 600, "scale": 2, "hz": 120, "view": { "preview": id } })).unwrap();
        let t = Ticked::new(a["service"].as_str().unwrap());
        if scenario == "hidden" {
            t.viewer.send("visible", json!({ "on": false })).unwrap();
        }
        preview_viewport = a["viewport"].as_u64();
        _preview = Some(t);
    }
    if scenario.starts_with("idle") {
        game.viewer.send("visible", json!({ "on": false })).unwrap();
    }
    if scenario == "idle-edit" {
        post(port, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    }
    // Warm up: pipelines compile, the model's textures upload; the stats keep the last 1000.
    std::thread::sleep(Duration::from_secs(4));
    let f0 = post(port, "bevy-ts/viewport_stats", Value::Null).unwrap()["frames"].as_u64().unwrap();
    let (_, cpu0) = ps(pid);
    std::thread::sleep(Duration::from_secs(secs));
    let (rss, cpu1) = ps(pid);
    let fp = footprint(pid);
    let g = post(port, "bevy-ts/viewport_stats", Value::Null).unwrap();
    let p = preview_viewport.map(|v| post(port, "bevy-ts/viewport_stats", json!({ "viewport": v })).unwrap());
    let fps = (g["frames"].as_u64().unwrap() - f0) as f64 / secs as f64;
    eprintln!("game stats {g}\npreview stats {p:?}");
    let ft = &g["frameTimes"];
    let row = format!(
        "| {scenario} | {} | {} | {} | {} | {fps:.0} | {} | {} | {:.0} | {:.0}% |",
        ms(&ft["cpuP50Ms"]),
        ms(&ft["cpuP99Ms"]),
        ms(&ft["gpuP50Ms"]),
        ms(&ft["gpuP99Ms"]),
        p.as_ref().map_or("-".into(), |p| ms(&p["frameTimes"]["gpuP50Ms"])),
        p.as_ref().map_or("-".into(), |p| format!("{:.0}", (p["frames"].as_u64().unwrap_or(0)) as f64)),
        rss as f64 / 1024.0,
        (cpu1 - cpu0) / secs as f64 * 100.0,
    );
    let gpu_mb = g["gpuAllocatedBytes"].as_u64().map_or("-".into(), |b| format!("{:.0}", b as f64 / 1048576.0));
    let row = format!("{row} {gpu_mb} | {fp:.0} |");
    println!("| scenario | update p50 | update p99 | game GPU p50 | game GPU p99 | game fps | preview GPU p50 | preview frames | RSS MB | CPU | GPU alloc MB | footprint MB |");
    println!("{row}");
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/preview-bench.md");
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(out).unwrap();
    writeln!(f, "{row}").unwrap();
}
