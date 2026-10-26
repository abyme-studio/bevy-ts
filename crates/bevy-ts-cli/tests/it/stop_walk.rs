//! Change `stop-incremental`, the walk: the real `bevy-ts dev --viewport` in a process of its own
//! (no window; the test stands in for the viewer, as `viewport_walk` does), a level with plain,
//! moved, despawned and animated-glTF entities and a level camera. Play, touch, Stop; a shot of
//! the game view at each step and the entity ids before and after.
//!
//! `cargo test -p bevy-ts-cli --test it stop_walk:: -- --ignored --nocapture`; shots in `target/stop-walk`.
#![cfg(target_os = "macos")]



use bevy_ts_viewport::mach::{Incoming, Viewer, release_port};
use bevy_ts_viewport::surface::Surface;
use crate::common::project;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const GAME: &str = r##"import { app, Component, SceneComponent, type Local, type Mut, type Commands, type Entity, type Query, Update, Transform, type u32, type With } from "bevy-ts";
import { PlayAnimation, WorldAssetRoot } from "bevy-ts/bevy";

export class Mover extends Component {}
export class Doomed extends Component {}

export class Enemy extends SceneComponent {
  static scene({ color = "#cc3333" }: { color?: string }) {
    return (
      <entity name="enemy">
        <mesh shape="cuboid" size={[1, 1, 1]} color={color} />
      </entity>
    );
  }
}
export class Walker extends SceneComponent {
  static scene() {
    return (
      <entity name="walker">
        <Mover />
        <mesh shape="cuboid" size={[1, 1, 1]} color="#3366cc" />
      </entity>
    );
  }
}
export class Victim extends SceneComponent {
  static scene() {
    return (
      <entity name="victim">
        <Doomed />
        <mesh shape="cuboid" size={[1, 1, 1]} color="#33aa33" />
      </entity>
    );
  }
}
export class Fox extends SceneComponent {
  static scene() {
    return (
      <entity name="fox">
        <entity key="model">
          <Transform scale={[0.02, 0.02, 0.02]} />
          <WorldAssetRoot value="models/Fox.glb#Scene0" />
          <PlayAnimation clip="models/Fox.glb#Animation2" />
        </entity>
      </entity>
    );
  }
}

/** Walkers jump up after a second; victims go after one and a half. */
export function play(clock: Local<{ frames: u32 }>, commands: Commands, movers: Query<[Mut<Transform>], With<Mover>>, doomed: Query<[Entity], With<Doomed>>) {
  clock.frames += 1;
  if (clock.frames > 60) movers.each((t) => { t.translation.y = 2.5; });
  if (clock.frames > 90) doomed.each((e) => { commands.entity(e).despawn(); });
}
app.addSystems(Update, play);
app.level("levels/walk.bsn");
"##;

const LEVEL: &str = "#Still
@game::Enemy
Transform { translation: Vec3 { x: -4.0, z: -2.0 } }
--
#Walker
@game::Walker
Transform { translation: Vec3 { x: -1.5, z: -2.0 } }
--
#Fox
@game::Fox
Transform { translation: Vec3 { x: 1.5, z: -2.0 } }
--
#Victim
@game::Victim
Transform { translation: Vec3 { x: 4.0, z: -2.0 } }
--
#Cam
Camera3d
Transform { translation: Vec3 { y: 3.0, z: 10.0 } }
--
#Lamp
PointLight { intensity: 3000000.0 }
Transform { translation: Vec3 { y: 6.0, z: 6.0 } }
--
#Sun
DirectionalLight
Transform { translation: Vec3 { y: 6.0, z: 6.0 }, rotation: Quat { x: -0.4, w: 0.9 } }
";

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
    let a = body.find('{').ok_or("no json")?;
    let b = body.rfind('}').ok_or("no json")? + 1;
    let v: Value = serde_json::from_str(&body[a..b]).map_err(|e| format!("{e}: {body}"))?;
    if let Some(e) = v.get("error") {
        return Err(format!("{method}: {e}"));
    }
    Ok(v["result"].clone())
}

#[derive(Default)]
struct Seen {
    surfaces: HashMap<(u64, u64), Surface>,
    last: Option<Value>,
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
            "frame" => seen.last = Some(params),
            _ => {}
        }
    }
}

/// The newest frame, saved as a PNG, and its RGBA bytes.
fn shot(viewer: &Viewer, seen: &mut Seen, dir: &Path, name: &str) -> Vec<u8> {
    std::thread::sleep(Duration::from_millis(400));
    seen.last = None;
    let t0 = Instant::now();
    while seen.last.is_none() {
        pump(viewer, seen, Duration::from_millis(20));
        assert!(t0.elapsed() < Duration::from_secs(10), "no frame for {name}");
    }
    let f = seen.last.clone().unwrap();
    let (w, h) = (f["w"].as_u64().unwrap() as u32, f["h"].as_u64().unwrap() as u32);
    let surface = &seen.surfaces[&(f["ring"].as_u64().unwrap(), f["slot"].as_u64().unwrap())];
    let points: Vec<(u32, u32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).collect();
    let mut rgba = Vec::with_capacity(points.len() * 4);
    for p in surface.read(&points) {
        rgba.extend_from_slice(&[p[2], p[1], p[0], 255]);
    }
    std::fs::write(dir.join(format!("{name}.png")), bevy_ts_viewport::assets::png(w, h, &rgba)).unwrap();
    rgba
}

fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 6)).count()
}

/// Entity ids by name.
fn ids(port: u16) -> BTreeMap<String, Vec<u64>> {
    let r = post(port, "bevy-ts/entities", Value::Null).unwrap();
    let mut out: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for e in r["entities"].as_array().unwrap() {
        if let Some(n) = e["name"].as_str()
            && ["Still", "Walker", "Fox", "Victim", "Cam", "Lamp", "Sun"].contains(&n)
        {
            out.entry(n.to_string()).or_default().push(e["entity"].as_u64().unwrap());
        }
    }
    out
}

fn wait_for(port: u16, what: &str, f: impl Fn(&BTreeMap<String, Vec<u64>>) -> bool) -> BTreeMap<String, Vec<u64>> {
    let t0 = Instant::now();
    loop {
        let now = ids(port);
        if f(&now) {
            return now;
        }
        assert!(t0.elapsed() < Duration::from_secs(30), "never: {what}: {now:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
#[ignore = "spawns the engine binary and renders; run by hand"]
fn the_stop_walk() {
    let fox = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/foxes/assets/models/Fox.glb")).unwrap();
    let dir = project("stop-walk", &[("src/game.tsx", GAME), ("assets/levels/walk.bsn", LEVEL), ("package.json", "{}")]);
    std::fs::create_dir_all(dir.join("assets/models")).unwrap();
    std::fs::write(dir.join("assets/models/Fox.glb"), fox).unwrap();
    let shots = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/stop-walk");
    std::fs::create_dir_all(&shots).unwrap();
    let log = std::fs::File::create(shots.join("engine.log")).unwrap();
    let port = 15900 + (std::process::id() % 40) as u16;
    let child = Command::new(env!("CARGO_BIN_EXE_bevy-ts"))
        .args(["dev", dir.join("src/game.tsx").to_str().unwrap(), "--viewport", "--remote-port", &port.to_string()])
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .spawn()
        .unwrap();
    let _engine = Engine(child);
    let t0 = Instant::now();
    while post(port, "bevy-ts/hello", Value::Null).is_err() {
        assert!(t0.elapsed() < Duration::from_secs(120), "the engine never answered");
        std::thread::sleep(Duration::from_millis(100));
    }
    let a = post(port, "bevy-ts/viewport_attach", json!({ "w": 800, "h": 600, "scale": 1, "hz": 60 })).unwrap();
    let viewer = Arc::new(Viewer::connect(a["service"].as_str().unwrap()).unwrap());
    let ticking = Arc::new(AtomicBool::new(true));
    let ticker = {
        let (viewer, ticking) = (viewer.clone(), ticking.clone());
        std::thread::spawn(move || {
            while ticking.load(Ordering::Relaxed) {
                let ts = bevy_ts_viewport::shared::now_ns();
                let _ = viewer.send("tick", json!({ "ts": ts, "targetTs": ts + 16_666_666 }));
                std::thread::sleep(Duration::from_micros(16_666));
            }
        })
    };
    viewer.send("input", json!({ "type": "focus", "on": true })).unwrap();
    let mut seen = Seen::default();

    let before = wait_for(port, "the level stands", |m| m.len() == 7);
    println!("level standing (the engine starts in a running game): {before:?}");
    // Edit view, the game camera on, so the shots are the level's camera.
    println!("mode: {}", post(port, "bevy-ts/viewport_mode", json!({ "mode": "edit", "camera": "game" })).unwrap());
    std::thread::sleep(Duration::from_secs(3));
    let before = ids(port);
    println!("edit baseline: {before:?}");
    let s0 = shot(&viewer, &mut seen, &shots, "1-edit-before");

    // 1: Play, touch nothing (shorter than the systems' 60 frames), Stop.
    println!("play: {}", post(port, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap());
    std::thread::sleep(Duration::from_millis(300));
    shot(&viewer, &mut seen, &shots, "2-play-short");
    println!("stop: {}", post(port, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap());
    let s1 = shot(&viewer, &mut seen, &shots, "3-after-quick-stop");
    let after1 = ids(port);
    println!("walk 1: ids after a quick Play/Stop: {after1:?}");
    println!("walk 1: pixels differing from before: {}", differing(&s0, &s1));
    for (n, v) in &after1 {
        println!("  {n:7} kept={} (before {:?} after {v:?})", before[n] == *v, before[n]);
    }

    // 2: Play long enough for the walker to jump and the victim to go.
    println!("play: {}", post(port, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap());
    std::thread::sleep(Duration::from_millis(3500));
    shot(&viewer, &mut seen, &shots, "4-play-long");
    let during = ids(port);
    println!("walk 2: during play: {during:?}");
    println!("stop: {}", post(port, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap());
    let s2 = shot(&viewer, &mut seen, &shots, "5-after-long-stop");
    let after2 = ids(port);
    println!("walk 2/3/5: ids after the long Play/Stop: {after2:?}");
    println!("walk 2: pixels differing from before: {}", differing(&s0, &s2));
    for (n, v) in &after2 {
        let was = &after1[n];
        println!("  {n:7} kept={} (before {was:?} after {v:?})", was == v);
    }
    // 3: a third, quick cycle: what a Play with nothing touched keeps once the level has been through two.
    post(port, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    post(port, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
    let s3 = shot(&viewer, &mut seen, &shots, "6-after-third-stop");
    let after3 = ids(port);
    println!("walk 3: pixels differing from the long stop: {}", differing(&s2, &s3));
    for (n, v) in &after3 {
        println!("  {n:7} kept={} (before {:?} after {v:?})", after2[n] == *v, after2[n]);
    }
    ticking.store(false, Ordering::Relaxed);
    ticker.join().unwrap();
}
