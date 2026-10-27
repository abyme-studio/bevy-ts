//! Change `scene-animation`, task 6: what `PlayAnimation` costs against Bevy's own Rust pattern.
//! One release engine per run (`examples/animation_bench_engine.rs`: the prebuilt engine plus a
//! probe), its game viewport ticked at 120 Hz as abyme's display link does, N copies of
//! `Fox.glb` in one arm:
//!
//! - `A`: a Rust plugin spawns the foxes the `many_foxes` way (own observer, graph, transitions);
//! - `B`: the foxes in TSX with `<PlayAnimation>`;
//! - `C`: the same TSX foxes with no `PlayAnimation` (static);
//! - `D`, a diagnostic: the foxes spawned by Rust as in A, played by `PlayAnimation`.
//!
//! Change `animation-control`'s arms, all B's TSX foxes:
//!
//! - `P`: `paused`, so parked;
//! - `H`: the camera turned away, so none in view, still playing;
//! - `K`: the camera turned away with `cull`, so parked;
//! - `G`: the floor, B with every player's `AnimationGraphHandle` removed by the engine once all play.
//!
//! Tweens (stage 2), N cubes each bobbing on a child named `box` (a ping-pong `CubicInOut` ease, 1 s
//! each way), no foxes:
//!
//! - `T`: one TS-built `AnimationClip`, played by `PlayAnimation` on every cube;
//! - `S`: a TS system writing every box's `Transform` each frame.
//!
//! `scripts/animation-bench.sh` builds the engine and runs the arms; one run is
//! `ANIMATION_BENCH_ENGINE=target/release/examples/animation_bench_engine ARM=B N=100 cargo static test --release -p bevy-ts-cli --test it animation_bench:: -- --ignored --nocapture`.
//! Each run appends a row to `target/animation-bench.md`: the probe's main-world update time
//! (mean, p95), spawn-to-ready (frames, ms), the game's GPU p50, CPU and RSS.
#![cfg(target_os = "macos")]

use crate::common;

use common::Engine;
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
    // A run that stalls fails rather than hangs the rest of the arms.
    s.set_read_timeout(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?;
    s.set_write_timeout(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?;
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

/// A viewer that says hello, ticks at 120 Hz and drains what it is sent, until dropped.
struct Ticked {
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
        Ticked { on }
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

/// Arms T and S: N cubes on the foxes' grid, each a root with a child `box` that bobs.
fn tweens(arm: &str, n: usize) -> String {
    let (play, system) = match arm {
        "T" => ("\n    <PlayAnimation clip={bobbing} repeat=\"Forever\" />", ""),
        _ => (
            "",
            r#"
/** `CubicInOut` up and back, as Bevy's ease and ping-pong compute it. */
const cubicInOut = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
export function bob(time: Res<Time>, boxes: Query<[Mut<Transform>], With<Bobs>>) {
  const t = time.elapsed_secs % 2;
  const y = cubicInOut(t < 1 ? t : 2 - t) * 2;
  boxes.each((b) => {
    b.translation.y = y;
  });
}
app.addSystems(Update, bob);
"#,
        ),
    };
    format!(
        r##"import {{ app, assets, type Commands, Component, type Mut, type Query, type Res, Startup, Time, Transform, Update, type With }} from "bevy-ts";
import {{ AnimatableCurve, animatedField, AnimationClip, AnimationTargetId, DirectionalLight, EasingCurve, interval, PlayAnimation }} from "bevy-ts/bevy";
const N = {n};
const side = Math.ceil(Math.sqrt(N));
const offset = ((side - 1) * 2.5) / 2;
const at = (i: number): [number, number, number] => [(i % side) * 2.5 - offset, 0, Math.floor(i / side) * 2.5 - offset];
export class Bobs extends Component {{}}
const up = new EasingCurve([0, 0, 0], [0, 2, 0], "CubicInOut").reparametrizeLinear(interval(0, 1)).pingPong();
const bobbing = assets.add(new AnimationClip().addCurveToTarget(AnimationTargetId.fromName("box"), new AnimatableCurve(animatedField(Transform, "translation"), up)));
export function stage(commands: Commands) {{
  commands.spawnNative("camera3d", {{ position: [0, side * 2, side * 3], lookAt: [0, 0, 0] }});
}}
const Cube = ({{ i }}: {{ i: number }}) => (
  <entity name="cube">
    <Transform translation={{at(i)}} />{play}
    <entity name="box">
      <Transform />
      <Bobs />
      <mesh shape="cuboid" size={{[1, 1, 1]}} color="#ff8800" />
    </entity>
  </entity>
);
app.scene("cubes", () => (
  <>
    <entity name="sun">
      <DirectionalLight shadow_maps_enabled />
      <Transform translation={{[3, 6, 2]}} rotation={{[-0.4341, 0.4107, 0.2323, 0.7674]}} />
    </entity>
    <entity name="ground"><mesh shape="plane" size={{[side * 3, side * 3]}} color="#4a6b3a" /></entity>
    {{Array.from({{ length: N }}, (_, i) => <Cube key={{`cube${{i}}`}} i={{i}} />)}}
  </>
));
app.addSystems(Startup, stage);{system}
"##
    )
}

/// The TSX scene: the camera, a light, a ground, and for arms B and C the foxes.
fn game(arm: &str, n: usize) -> String {
    if arm == "T" || arm == "S" {
        return tweens(arm, n);
    }
    let play = match arm {
        "B" | "H" | "G" => "\n    <PlayAnimation clip=\"models/Fox.glb#Animation2\" />",
        "P" => "\n    <PlayAnimation clip=\"models/Fox.glb#Animation2\" paused />",
        "K" => "\n    <PlayAnimation clip=\"models/Fox.glb#Animation2\" cull />",
        _ => "",
    };
    let foxes = if arm == "A" || arm == "D" { "0" } else { "N" };
    // H and K look away from the grid, so no fox is in view.
    let look = if arm == "H" || arm == "K" { "[0, side * 2, side * 6]" } else { "[0, 0, 0]" };
    format!(
        r##"import {{ app, type Commands, Startup, Transform }} from "bevy-ts";
import {{ Camera3d, DirectionalLight, PlayAnimation, WorldAssetRoot }} from "bevy-ts/bevy";
const N = {n};
const side = Math.ceil(Math.sqrt(N));
const offset = ((side - 1) * 2.5) / 2;
const at = (i: number): [number, number, number] => [(i % side) * 2.5 - offset, 0, Math.floor(i / side) * 2.5 - offset];
export function stage(commands: Commands) {{
  commands.spawn(Camera3d, Transform.lookingAt([0, side * 2, side * 3], {look}));
}}
const Fox = ({{ i }}: {{ i: number }}) => (
  <entity name="fox">
    <Transform translation={{at(i)}} scale={{[0.02, 0.02, 0.02]}} />
    <WorldAssetRoot value="models/Fox.glb#Scene0" />{play}
  </entity>
);
app.scene("foxes", () => (
  <>
    <entity name="sun">
      <DirectionalLight shadow_maps_enabled />
      <Transform translation={{[3, 6, 2]}} rotation={{[-0.4341, 0.4107, 0.2323, 0.7674]}} />
    </entity>
    <entity name="ground"><mesh shape="plane" size={{[side * 3, side * 3]}} color="#4a6b3a" /></entity>
    {{Array.from({{ length: {foxes} }}, (_, i) => <Fox key={{`fox${{i}}`}} i={{i}} />)}}
  </>
));
app.addSystems(Startup, stage);
"##
    )
}

#[test]
#[ignore = "a measurement: runs the release engine for several seconds; see the header"]
fn animation_cost() {
    let arm = std::env::var("ARM").unwrap_or_else(|_| "B".into());
    let n: usize = std::env::var("N").ok().and_then(|s| s.parse().ok()).unwrap_or(100);
    let engine = std::env::var("ANIMATION_BENCH_ENGINE").expect("ANIMATION_BENCH_ENGINE: the built animation_bench_engine example");
    let dir = crate::common::TestDir::new("animation-bench");
    std::fs::create_dir_all(dir.join("assets/models")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/foxes/assets/models/Fox.glb"), dir.join("assets/models/Fox.glb")).unwrap();
    std::fs::write(dir.join("src/game.tsx"), game(&arm, n)).unwrap();
    // Marks the project root, so `assets/` beside it is the asset root.
    std::fs::write(dir.join("tsconfig.json"), "{}").unwrap();
    let out = dir.join("probe.json");

    let port = crate::common::free_port();
    let guard = Engine::spawn(
        Command::new(&engine)
            .args(["dev", dir.join("src/game.tsx").to_str().unwrap(), "--viewport", "--remote-port", &port.to_string()])
            .env("ANIMATION_BENCH_ARM", &arm)
            .env("ANIMATION_BENCH_N", n.to_string())
            .env("ANIMATION_BENCH_OUT", &out)
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    );
    let pid = guard.id();
    let t0 = Instant::now();
    while post(port, "bevy-ts/hello", Value::Null).map(|h| h["generation"].as_u64().unwrap_or(0)).unwrap_or(0) == 0 {
        assert!(t0.elapsed() < Duration::from_secs(120), "the scene never loaded");
        std::thread::sleep(Duration::from_millis(100));
    }
    let game = post(port, "bevy-ts/viewport_attach", json!({ "w": 1280, "h": 720, "scale": 2, "hz": 120 })).unwrap();
    let _game = Ticked::new(game["service"].as_str().unwrap());
    let (_, cpu0) = ps(pid);
    let t1 = Instant::now();
    while !out.exists() {
        assert!(t1.elapsed() < Duration::from_secs(180), "the probe never wrote");
        std::thread::sleep(Duration::from_millis(100));
    }
    let secs = t1.elapsed().as_secs_f64();
    let (rss, cpu1) = ps(pid);
    let probe: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let g = post(port, "bevy-ts/viewport_stats", Value::Null).unwrap();
    eprintln!("probe {probe}\ngame stats {g}");
    let f = |k: &str| probe[k].as_f64().map_or("-".into(), |x| format!("{x:.3}"));
    let ready = if arm == "C" { "-".into() } else { format!("{} | {}", probe["readyFrames"], f("readyMs")) };
    let ready = if arm == "C" { format!("{ready} | -") } else { ready };
    let row = format!(
        "| {arm} | {n} | {} | {} | {} | {ready} | {} | {:.0}% | {:.0} |",
        probe["players"],
        f("meanMs"),
        f("p95Ms"),
        g["frameTimes"]["gpuP50Ms"].as_f64().map_or("-".into(), |x| format!("{x:.2}")),
        (cpu1 - cpu0) / secs * 100.0,
        rss as f64 / 1024.0,
    );
    println!("| arm | N | players | update mean ms | update p95 ms | ready frames | ready ms | game GPU p50 | CPU | RSS MB |");
    println!("{row}");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/animation-bench.md");
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
    writeln!(file, "{row}").unwrap();
}
