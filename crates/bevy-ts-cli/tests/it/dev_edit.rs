//! Change `dev-edit`: `bevy-ts dev --edit` boots in edit mode. The arena under `--viewport --edit`
//! in a process of its own shows its level, lit, from the editor camera with no game code run; the
//! first play starts it. `--edit --headless` is refused.

use crate::common;

use common::BIN;
use std::process::Command;

#[test]
fn edit_and_headless_cannot_be_combined() {
    let cube = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cube/src/game.ts");
    let out = Command::new(BIN).args(["dev", "--edit", "--headless", cube.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--edit and --headless cannot be combined"), "{err}");
}

#[cfg(target_os = "macos")]
mod arena {
    use bevy_ts_viewport::mach::{Incoming, Viewer, release_port};
    use serde_json::{Value, json};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

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

    /// Waits for `n` more frames from the viewer.
    fn frames(viewer: &Viewer, seen: &mut usize, n: usize) {
        let want = *seen + n;
        let t0 = Instant::now();
        while *seen < want {
            assert!(t0.elapsed() < Duration::from_secs(60), "no frames: {seen} of {want}");
            if let Some(Incoming::Message { method, port, .. }) = viewer.recv(Duration::from_millis(20)) {
                if let Some(port) = port {
                    release_port(port);
                }
                if method == "frame" {
                    *seen += 1;
                }
            }
        }
    }

    /// Every enemy's local translation, sorted.
    fn enemies(port: u16) -> Vec<[f64; 3]> {
        let r = post(
            port,
            "world.query",
            json!({ "data": { "components": ["bevy_ecs::name::Name", "bevy_transform::components::transform::Transform"] } }),
        )
        .unwrap();
        let mut out: Vec<[f64; 3]> = r
            .as_array()
            .unwrap()
            .iter()
            // The level names its enemies; each room names its guard.
            .filter(|e| ["EnemyA", "EnemyB", "Guard"].iter().any(|n| e["components"]["bevy_ecs::name::Name"] == *n))
            .map(|e| {
                let t = &e["components"]["bevy_transform::components::transform::Transform"]["translation"];
                [t[0].as_f64().unwrap(), t[1].as_f64().unwrap(), t[2].as_f64().unwrap()]
            })
            .collect();
        out.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out
    }

    fn count(port: u16, component: &str) -> usize {
        post(port, "world.query", json!({ "data": { "components": [component] } })).unwrap().as_array().unwrap().len()
    }

    #[test]
    fn the_arena_opens_in_edit_mode_and_the_first_play_starts_it() {
        let arena = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena/src/game.tsx");
        let log = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dev-edit-engine.log");
        let port = crate::common::free_port();
        let child = Command::new(env!("CARGO_BIN_EXE_bevy-ts"))
            .args(["dev", arena.to_str().unwrap(), "--viewport", "--edit", "--remote-port", &port.to_string()])
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
        let a = post(port, "bevy-ts/viewport_attach", json!({ "w": 400, "h": 300, "scale": 1, "hz": 120 })).unwrap();
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
        let mut seen = 0;
        // The level loads on the first frames.
        let t0 = Instant::now();
        while enemies(port).len() < 4 {
            assert!(t0.elapsed() < Duration::from_secs(60), "the level never loaded: {:?}", enemies(port));
            frames(&viewer, &mut seen, 5);
        }
        frames(&viewer, &mut seen, 30);

        let m = post(port, "bevy-ts/viewport_mode", json!({})).unwrap();
        assert_eq!((m["mode"].as_str(), m["started"].as_bool()), (Some("edit"), Some(false)));
        let systems = post(port, "bevy-ts/systems", Value::Null).unwrap();
        let ran: Vec<&Value> = systems.as_array().unwrap().iter().filter(|s| s["lastRunMicros"].as_u64().unwrap_or(0) > 0).collect();
        assert!(ran.is_empty(), "no TS system runs in edit: {ran:?}");
        // At their level transforms: the arena's two, and each room's guard (local to its room).
        assert_eq!(enemies(port), vec![[-3.0, 0.0, -4.0], [0.0, 0.0, -16.0], [0.0, 0.0, -16.0], [3.0, 0.0, -4.0]]);
        assert!(count(port, "bevy_light::directional_light::DirectionalLight") >= 1, "the sun is the scene's, so edit shows it");
        let cam = post(port, "bevy-ts/viewport_camera", Value::Null).unwrap();
        assert_eq!(cam["translation"], json!([-6.0, 5.0, 10.0]), "the editor camera's first pose");

        let r = post(port, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        assert_eq!((r["mode"].as_str(), r["started"].as_bool()), (Some("play"), Some(true)));
        frames(&viewer, &mut seen, 30);
        assert!(enemies(port).iter().any(|t| t[1].abs() > 1e-3), "the enemies bob once the game runs: {:?}", enemies(port));
        ticking.store(false, Ordering::Relaxed);
        ticker.join().unwrap();
        let log = std::fs::read_to_string(&log).unwrap();
        assert!(!log.contains("panicked"), "the engine panicked:\n{log}");
    }
}
