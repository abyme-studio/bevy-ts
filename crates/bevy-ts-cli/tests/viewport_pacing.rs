//! The viewport's frame pacing (docs/research/viewport-pacing.md): the arena under `bevy-ts dev
//! --viewport` in edit mode, ticked at 120 Hz as a display link would, at rest, under keys and
//! under a pointer drag. Prints frames a second, skips, dropped ticks, CPU and GPU time and input
//! to frame for each phase. `BEVY_TS_BINARY` picks the engine (the release build, to measure);
//! `BEVY_TS_VIEWPORT_TRACE` is passed through for the per-frame timeline.
//!
//! `cargo test --release -p bevy-ts-cli --test viewport_pacing -- --ignored --nocapture`
#![cfg(target_os = "macos")]

use bevy_ts_viewport::mach::{Incoming, Viewer, release_port};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

unsafe extern "C" {
    fn pthread_set_qos_class_self_np(qos: u32, relative_priority: i32) -> i32;
}

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
    if let Some(e) = v.get("error") {
        return Err(format!("{method}: {e}"));
    }
    Ok(v["result"].clone())
}

fn n(v: &Value, k: &str) -> u64 {
    v[k].as_u64().unwrap_or(0)
}

/// One phase's numbers, from the stats before and after and the frames the viewer saw.
fn report(name: &str, before: &Value, after: &Value, secs: f64, gaps: &mut Vec<f64>) {
    gaps.sort_by(f64::total_cmp);
    let at = |q: f64| gaps.get(((gaps.len().max(1) - 1) as f64 * q).round() as usize).copied().unwrap_or(0.0);
    let ft = &after["frameTimes"];
    let lat = &after["inputToFrame"];
    println!(
        "{name}: {:.0} fps, {} skipped, {} dropped ticks; frame gap p50 {:.1} p95 {:.1} max {:.1} ms; cpu p50 {:.2} p99 {:.2} ms, gpu p50 {:.2} p99 {:.2} ms; input to frame median {:.1} p95 {:.1} ms ({} samples)",
        (n(after, "frames") - n(before, "frames")) as f64 / secs,
        n(after, "skipped") - n(before, "skipped"),
        n(after, "droppedTicks") - n(before, "droppedTicks"),
        at(0.5),
        at(0.95),
        gaps.last().copied().unwrap_or(0.0),
        ft["cpuP50Ms"].as_f64().unwrap_or(0.0),
        ft["cpuP99Ms"].as_f64().unwrap_or(0.0),
        ft["gpuP50Ms"].as_f64().unwrap_or(0.0),
        ft["gpuP99Ms"].as_f64().unwrap_or(0.0),
        lat["medianMs"].as_f64().unwrap_or(0.0),
        lat["p95Ms"].as_f64().unwrap_or(0.0),
        n(lat, "samples"),
    );
    gaps.clear();
}

/// Frame arrival gaps in ms since the last call.
fn pump(viewer: &Viewer, last: &mut Option<Instant>, gaps: &mut Vec<f64>, wait: Duration) -> usize {
    let mut frames = 0;
    while let Some(m) = viewer.recv(wait) {
        let Incoming::Message { method, port, .. } = m else { continue };
        if let Some(p) = port {
            release_port(p);
        }
        if method == "frame" {
            let now = Instant::now();
            if let Some(l) = *last {
                gaps.push(now.duration_since(l).as_secs_f64() * 1000.0);
            }
            *last = Some(now);
            frames += 1;
        }
    }
    frames
}

/// Runs `f` while pumping the viewer, for `secs`.
fn phase(viewer: &Viewer, port: u16, name: &str, secs: f64, mut f: impl FnMut(f64)) {
    let (mut last, mut gaps) = (None, vec![]);
    pump(viewer, &mut last, &mut vec![], Duration::ZERO);
    let before = post(port, "bevy-ts/viewport_stats", Value::Null).unwrap();
    let t0 = Instant::now();
    while t0.elapsed().as_secs_f64() < secs {
        f(t0.elapsed().as_secs_f64());
        pump(viewer, &mut last, &mut gaps, Duration::from_millis(1));
    }
    let after = post(port, "bevy-ts/viewport_stats", Value::Null).unwrap();
    report(name, &before, &after, t0.elapsed().as_secs_f64(), &mut gaps);
}

#[test]
#[ignore = "timing: spawns the engine and needs a release build and a quiet machine"]
fn the_arena_in_edit_mode_at_120_hz() {
    let dir = std::env::temp_dir().join(format!("bevy-ts-viewport-pacing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let arena = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena");
    assert!(Command::new("cp").args(["-R", arena.to_str().unwrap(), dir.to_str().unwrap()]).status().unwrap().success());
    let bin = std::env::var("BEVY_TS_BINARY").unwrap_or_else(|_| env!("CARGO_BIN_EXE_bevy-ts").to_string());
    let port = 15900 + (std::process::id() % 40) as u16;
    let child = Command::new(&bin)
        .args(["dev", dir.join("src/game.tsx").to_str().unwrap(), "--viewport", "--remote-port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    let _engine = Engine(child);
    let t0 = Instant::now();
    while post(port, "bevy-ts/hello", Value::Null).is_err() {
        assert!(t0.elapsed() < Duration::from_secs(60), "the engine never answered");
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("engine {bin}");

    // The bevy-studio walk's hole: 480x320 points at 2x on a 120 Hz display.
    let a = post(port, "bevy-ts/viewport_attach", json!({ "w": 480, "h": 320, "scale": 2, "hz": 120 })).unwrap();
    let viewer = Arc::new(Viewer::connect(a["service"].as_str().unwrap()).unwrap());
    let ticking = Arc::new(AtomicBool::new(true));
    let ticker = {
        let (viewer, ticking) = (viewer.clone(), ticking.clone());
        std::thread::spawn(move || {
            // A display link's thread is high priority: a default one jitters on a busy machine.
            // (Not `runner::interactive`, which `BEVY_TS_VIEWPORT_QOS=0` turns off for the engine.)
            unsafe { pthread_set_qos_class_self_np(0x21, 0) };
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
    viewer.send("input", json!({ "type": "focus", "on": true })).unwrap();
    let (mut last, mut frames) = (None, 0);
    let t0 = Instant::now();
    while frames < 10 {
        frames += pump(&viewer, &mut last, &mut vec![], Duration::from_millis(20));
        assert!(t0.elapsed() < Duration::from_secs(60), "no frames");
    }
    phase(&viewer, port, "play at rest", 2.0, |_| {});
    post(port, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    phase(&viewer, port, "edit settling", 1.0, |_| {});
    // `PACING_SAMPLE=<file>`: a call-stack profile of the engine over this phase.
    let sampler = std::env::var("PACING_SAMPLE").ok().map(|f| Command::new("sample").args([&pid.to_string(), "3", "-file", &f]).stdout(Stdio::null()).spawn().unwrap());
    phase(&viewer, port, "edit at rest", 3.0, |_| {});
    if let Some(mut s) = sampler {
        let _ = s.wait();
    }

    // Keys at spread phases of the tick, about 6 a second.
    let mut k = 0u64;
    let mut next = 0.0;
    phase(&viewer, port, "edit keys", 3.0, |t| {
        if t >= next {
            let state = if k % 2 == 0 { "down" } else { "up" };
            viewer.send("input", json!({ "type": "key", "state": state, "code": "KeyQ" })).unwrap();
            k += 1;
            next = t + 0.083 + (k * 1777 % 8333) as f64 / 1e6;
        }
    });

    // A pointer drag across the hole, a move every ~4 ms as a trackpad sends them.
    let mut i = 0u64;
    let mut next = 0.0;
    phase(&viewer, port, "edit pointer moves", 3.0, |t| {
        if t >= next {
            viewer.send("input", json!({ "type": "pointer", "kind": "move", "x": 40 + i % 400, "y": 160 })).unwrap();
            i += 1;
            next = t + 0.004;
        }
    });
    println!("stats: {}", post(port, "bevy-ts/viewport_stats", Value::Null).unwrap());
    ticking.store(false, Ordering::Relaxed);
    ticker.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
