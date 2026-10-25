//! Stage 3 of bevy-ui-coverage: `onPointerDragStart`, `onPointerDrag` and `onPointerDragEnd` on
//! any picked entity, through Bevy's own picking under `bevy-ts dev --headless`: a UI node, a
//! mesh, and a glTF scene's root, which hears a drag on its mesh as it hears a click (mesh-picking).
//! Bevy 0.20 starts a drag on the first move while pressed, with no threshold: a 2 px move is a
//! drag with distance 2, and a threshold is the game's, from `distance`.

use crate::common;

use common::{BOUND, Engine, LAUNCH};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

const GAME: &str = r##"import { app, px } from "bevy-ts";
import { Camera3d, Transform, WorldAssetRoot } from "bevy-ts/bevy";

const log = (who: string) => ({
  onPointerDragStart: () => console.log(`${who} drag-start`),
  onPointerDrag: ({ distance }: { distance: readonly [number, number] }) => console.log(`${who} drag ${Math.round(distance[0])},${Math.round(distance[1])}`),
  onPointerDragEnd: ({ distance }: { distance: readonly [number, number] }) => console.log(`${who} drag-end ${Math.round(distance[0])},${Math.round(distance[1])}`),
});

app.scene("s", () => (
  <>
    <entity key="camera">
      <Camera3d />
      <Transform translation={[0, 0, 6]} />
    </entity>
    <entity key="cube" {...log("cube")}>
      <Transform translation={[-1.5, 0, 0]} />
      <mesh shape="cuboid" size={[1, 1, 1]} color="#ff8800" />
    </entity>
    <entity key="crate" {...log("crate")}>
      <Transform translation={[1.5, 0, 0]} />
      <WorldAssetRoot value="models/crate.gltf#Scene0" />
    </entity>
    <node key="pad" style={{ position: "absolute", left: px(0), top: px(0), width: px(150), height: px(150) }} background="#2c3444" {...log("pad")} />
  </>
));
"##;

fn post(port: u16, method: &str, params: Value) -> Result<Value, String> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(BOUND)).map_err(|e| e.to_string())?;
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

fn pointer(kind: &str, x: f64, y: f64) -> Value {
    json!({ "type": "pointer", "kind": kind, "x": x, "y": y, "button": "left" })
}

/// Presses at (x, y), moves `dx` pixels right, releases; returns the drag lines `who` logged.
fn drag(port: u16, rx: &Receiver<String>, who: &str, x: f64, y: f64, dx: f64) -> Vec<String> {
    let events = json!([pointer("move", x, y), pointer("down", x, y), pointer("move", x + dx, y), pointer("up", x + dx, y)]);
    post(port, "bevy-ts/viewport_input", json!({ "events": events })).unwrap();
    let lines = common::wait_for(rx, &[&format!("{who} drag-end")], BOUND);
    std::thread::sleep(Duration::from_millis(200));
    lines.into_iter().chain(rx.try_iter()).filter(|l| l.contains(" drag")).map(|l| {
            // console.log lines end with their source location, " (file:line:col)".
            let l = &l[l.find(who).unwrap_or(0)..];
            l[..l.find(" (").unwrap_or(l.len())].to_string()
        }).collect()
}

#[test]
fn a_drag_reaches_a_ui_node_a_mesh_and_a_scene_root() {
    let model = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena/assets/models/crate.gltf")).unwrap();
    let dir = common::project(
        "ui-drag",
        &[("bevy-ts.json", r#"{ "meshPicking": true }"#), ("tsconfig.json", "{}"), ("src/game.tsx", GAME), ("assets/models/crate.gltf", &model)],
    );
    let port = common::free_port();
    let mut engine = Engine::spawn(
        Command::new(common::BIN)
            .args(["dev", dir.join("src/game.tsx").to_str().unwrap(), "--headless", "--remote-port", &port.to_string()])
            .env("NO_COLOR", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::piped()),
    );
    let (tx, rx) = channel();
    let err = engine.child().stderr.take().unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(err).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    let t0 = Instant::now();
    while post(port, "bevy-ts/hello", Value::Null).is_err() {
        assert!(t0.elapsed() < LAUNCH, "the engine never answered");
        std::thread::sleep(Duration::from_millis(50));
    }
    common::wait_for(&rx, &["generation 1 applied"], LAUNCH);
    // Frames for the camera, layout, the crate's scene and the meshes' bounds.
    for _ in 0..20 {
        post(port, "bevy-ts/hello", Value::Null).unwrap();
    }

    // 960×540 image, 45° field of view at 6 units: 108.6 px a unit, so x = ±1.5 is 163 px off centre.
    for (who, x, y) in [("pad", 75.0, 75.0), ("cube", 317.0, 270.0), ("crate", 643.0, 270.0)] {
        let short = drag(port, &rx, who, x, y, 2.0);
        assert_eq!(short, [format!("{who} drag-start"), format!("{who} drag 2,0"), format!("{who} drag-end 2,0")], "{who}: a 2 px move is a drag; Bevy has no threshold");
        let long = drag(port, &rx, who, x, y, 12.0);
        assert_eq!(long, [format!("{who} drag-start"), format!("{who} drag 12,0"), format!("{who} drag-end 12,0")], "{who}: a 12 px move");
    }
}
