//! Change `mesh-picking`: under `bevy-ts dev --headless`, `bevy-ts/viewport_input` drives a pointer
//! on the offscreen image, so a click on a cube reaches its `onPointerClick` through Bevy's mesh
//! picking, switched on by `"meshPicking": true`. Other input types are refused. Every platform.

use crate::common;

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

const GAME: &str = r##"import { app, Startup } from "bevy-ts";
import type { Commands } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";

export function camera(commands: Commands) {
  commands.spawnNative("camera3d", { position: [0, 0, 4], lookAt: [0, 0, 0] });
}
app.addSystems(Startup, camera);

app.scene("s", () => (
  <entity name="cube" onPointerClick={({ button }) => console.log(`cube clicked ${button}`)}>
    <Transform />
    <mesh shape="cuboid" size={[1, 1, 1]} color="#ff8800" />
  </entity>
));
"##;

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

/// `bevy-ts dev --headless` on the cube project, its stderr lines on a channel, once it answers.
fn dev(name: &str, config: &str) -> (Engine, Receiver<String>, u16) {
    let dir = common::project(name, &[("bevy-ts.json", config), ("tsconfig.json", "{}"), ("src/game.tsx", GAME)]);
    let port = common::free_port();
    let mut child = Command::new(common::BIN)
        .args(["dev", dir.join("src/game.tsx").to_str().unwrap(), "--headless", "--remote-port", &port.to_string()])
        .env("NO_COLOR", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (tx, rx) = channel();
    let err = child.stderr.take().unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(err).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    let engine = Engine(child);
    let t0 = Instant::now();
    while post(port, "bevy-ts/hello", Value::Null).is_err() {
        assert!(t0.elapsed() < Duration::from_secs(60), "the engine never answered");
        std::thread::sleep(Duration::from_millis(50));
    }
    (engine, rx, port)
}

fn pointer(kind: &str, x: f64, y: f64) -> Value {
    json!({ "type": "pointer", "kind": kind, "x": x, "y": y, "button": "left" })
}

#[test]
fn a_test_clicks_a_mesh_headless() {
    let (_engine, rx, port) = dev("headless-pointer-click", r#"{ "meshPicking": true }"#);
    common::wait_for(&rx, &["generation 1 applied"], Duration::from_secs(60));
    // A few frames for the camera and the cube's bounds.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(post(port, "bevy-ts/hello", Value::Null).unwrap()["viewport"], false);

    // The cube fills the middle of the 960×540 image.
    let (x, y) = (480.0, 270.0);
    let r = post(port, "bevy-ts/viewport_input", json!({ "events": [pointer("move", x, y), pointer("down", x, y), pointer("up", x, y)] })).unwrap();
    assert_eq!(r, json!({ "queued": 3 }));
    let lines = common::wait_for(&rx, &["cube clicked"], Duration::from_secs(10));
    std::thread::sleep(Duration::from_millis(300));
    let clicks = lines.iter().chain(rx.try_iter().collect::<Vec<_>>().iter()).filter(|l| l.contains("cube clicked")).count();
    assert_eq!(clicks, 1);
    assert!(lines.iter().any(|l| l.contains("cube clicked primary")), "{lines:?}");

    // A key is refused, naming it, and nothing of the request applies: the pointer in it would
    // otherwise click again.
    let e = post(port, "bevy-ts/viewport_input", json!({ "events": [pointer("down", x, y), pointer("up", x, y), { "type": "key", "code": "KeyA", "state": "down" }] }))
        .unwrap_err();
    assert!(e.contains("key"), "{e}");
    let e = post(port, "bevy-ts/viewport_input", json!({ "events": [pointer("move", x, y)], "viewport": 1 })).unwrap_err();
    assert!(e.contains("viewport 1"), "{e}");
    std::thread::sleep(Duration::from_millis(300));
    assert!(!rx.try_iter().any(|l| l.contains("cube clicked")), "a refused request applied");
    // The rest of the viewport stays unserved headless.
    assert!(post(port, "bevy-ts/viewport_attach", json!({ "w": 10, "h": 10, "scale": 1 })).unwrap_err().contains("viewport_attach"));
}

#[test]
fn a_wrong_mesh_picking_type_fails_dev() {
    let dir = common::project("headless-pointer-wrong", &[("bevy-ts.json", r#"{ "meshPicking": "yes" }"#), ("tsconfig.json", "{}"), ("src/game.tsx", GAME)]);
    let out = Command::new(common::BIN).args(["dev", dir.join("src/game.tsx").to_str().unwrap(), "--headless"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("meshPicking") && err.contains("bevy-ts.json"), "{err}");
}
