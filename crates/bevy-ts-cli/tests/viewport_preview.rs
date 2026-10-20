//! Change `asset-preview`: a preview never reaches the game's TS or the entity list, and an engine
//! whose TS fails on the first load still serves the protocol and previews, then runs the game
//! once the file is fixed.
#![cfg(target_os = "macos")]

mod common;

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_v8::V8Options;
use common::{messages, project, reload, write};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A one-mesh glTF: a unit cube, buffer inline.
fn write_gltf(path: &Path) {
    let p: [[f32; 3]; 8] = [[-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5], [-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]];
    let idx: [u16; 36] = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7, 3, 1, 2, 6, 1, 6, 5];
    let mut buf = vec![];
    for c in p.iter().flatten() {
        buf.extend_from_slice(&c.to_le_bytes());
    }
    for i in idx {
        buf.extend_from_slice(&i.to_le_bytes());
    }
    let uri = format!("data:application/octet-stream;base64,{}", bevy_ts_viewport::assets::base64(&buf));
    let doc = json!({
        "asset": { "version": "2.0" },
        "scene": 0, "scenes": [{ "nodes": [0] }], "nodes": [{ "name": "grunt", "mesh": 0 }],
        "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 }, "indices": 1 }] }],
        "buffers": [{ "byteLength": buf.len(), "uri": uri }],
        "bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": 96 }, { "buffer": 0, "byteOffset": 96, "byteLength": 72 }],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 8, "type": "VEC3", "min": [-0.5, -0.5, -0.5], "max": [0.5, 0.5, 0.5] },
            { "bufferView": 1, "componentType": 5123, "count": 36, "type": "SCALAR" }
        ]
    });
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, doc.to_string()).unwrap();
}

const COUNTER: &str = r#"import { app, Update, Transform } from "bevy-ts";
import type { Query } from "bevy-ts";
export function count(q: Query<[Transform]>) { console.log(`transforms ${q.len}`); }
app.addSystems(Update, count);
"#;

fn call(app: &mut App, method: &str, params: Value) -> Result<Value, String> {
    let id = match app.world().resource::<RemoteMethods>().get(method) {
        Some(RemoteMethodSystemId::Instant(id)) => *id,
        _ => panic!("{method} not served"),
    };
    app.world_mut().run_system_with(id, (!params.is_null()).then_some(params)).unwrap().map_err(|e| e.message)
}

fn watch(app: &mut App, method: &str) -> Option<Value> {
    let id = match app.world().resource::<RemoteMethods>().get(method) {
        Some(RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("{method} not served"),
    };
    app.world_mut().run_system_with(id, None).unwrap().unwrap()
}

#[test]
fn preview_entities_never_reach_a_ts_query_or_the_entity_list() {
    let dir = project("viewport-preview-hidden", &[("game.ts", COUNTER)]);
    write_gltf(&dir.join("assets/models/grunt.gltf"));
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: dir.join("assets").display().to_string(), ..default() }),
    )
    .add_plugins(RemotePlugin::default())
    .add_plugins(bevy_ts_viewport::ViewportPlugin { windowed: false })
    .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin)
    .insert_resource(bevy_ts_viewport::assets::AssetRoot(dir.join("assets")))
    .insert_resource(DevSource(Arc::new(Compiler::new(&dir.join("game.ts"), true).unwrap())));
    bevy_ts_remote::add_methods(&mut app);
    app.finish();
    app.cleanup();
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    for _ in 0..3 {
        app.update();
    }
    let snapshot = watch(&mut app, "bevy-ts/entities+watch").unwrap();
    let before = messages(&app).iter().rev().find(|m| m.starts_with("transforms")).cloned().unwrap_or_else(|| panic!("{:?}", messages(&app)));

    let t0 = Instant::now();
    let id = loop {
        let r = call(&mut app, "bevy-ts/asset_preview", json!({ "path": "models/grunt.gltf" })).unwrap();
        if let Some(id) = r["preview"].as_u64() {
            break id;
        }
        assert!(t0.elapsed() < Duration::from_secs(60), "the preview never loaded");
        std::thread::sleep(Duration::from_millis(5));
        app.update();
    };
    for _ in 0..6 {
        app.update();
    }
    let previews = app.world_mut().query_filtered::<(), (With<bevy_ts_viewport::preview::PreviewOnly>, With<Transform>)>().iter(app.world()).count();
    assert!(previews >= 5, "the preview scene spawned: {previews}");
    let after = messages(&app).iter().rev().find(|m| m.starts_with("transforms")).cloned().unwrap();
    assert_eq!(before, after, "the TS query sees none of the preview's {previews} entities");

    let names = |v: &Value, key: &str| -> Vec<String> { v[key].as_array().into_iter().flatten().filter_map(|e| e["name"].as_str().map(String::from)).collect() };
    assert!(!names(&snapshot, "snapshot").iter().any(|n| n.contains("preview")));
    if let Some(delta) = watch(&mut app, "bevy-ts/entities+watch") {
        assert!(delta["spawned"].as_array().into_iter().flatten().all(|e| !e["name"].as_str().unwrap_or("").contains("preview")), "{delta:#}");
    }
    let tree = call(&mut app, "bevy-ts/tsx_tree", Value::Null).unwrap();
    assert!(!tree.to_string().contains("preview"), "{tree}");
    assert_eq!(call(&mut app, "bevy-ts/asset_preview_close", json!({ "preview": id })).unwrap()["closed"], true);
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
    let start = out.find("\r\n\r\n").ok_or("no body")? + 4;
    let body = &out[start..];
    let (a, b) = (body.find('{').ok_or("no json")?, body.rfind('}').ok_or("no json")? + 1);
    let v: Value = serde_json::from_str(&body[a..b]).map_err(|e| format!("{e}: {body}"))?;
    if let Some(e) = v.get("error") {
        return Err(format!("{method}: {e}"));
    }
    Ok(v["result"].clone())
}

/// Calls `method` until `done` holds of its answer.
fn until(port: u16, method: &str, params: Value, what: &str, done: impl Fn(&Value) -> bool) -> Value {
    let t0 = Instant::now();
    loop {
        if let Ok(v) = post(port, method, params.clone())
            && done(&v)
        {
            return v;
        }
        assert!(t0.elapsed() < Duration::from_secs(90), "{what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A `+watch` stream (server-sent events), read one message at a time.
struct Stream {
    s: TcpStream,
    buf: String,
}

impl Stream {
    fn open(port: u16, method: &str) -> Stream {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method }).to_string();
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
        let req = format!("POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        s.write_all(req.as_bytes()).unwrap();
        Stream { s, buf: String::new() }
    }

    /// Messages until one satisfies `done`.
    fn until(&mut self, what: &str, done: impl Fn(&Value) -> bool) -> Value {
        let mut chunk = [0u8; 65536];
        loop {
            if let Some(i) = self.buf.find("data: ")
                && let Some(j) = self.buf[i..].find("\n\n")
            {
                let v: Value = serde_json::from_str(&self.buf[i + 6..i + j]).unwrap();
                self.buf.drain(..i + j + 2);
                if done(&v["result"]) {
                    return v["result"].clone();
                }
                continue;
            }
            let n = self.s.read(&mut chunk).unwrap_or_else(|e| panic!("{what}: {e}"));
            assert!(n > 0, "{what}: the stream ended");
            self.buf.push_str(&String::from_utf8_lossy(&chunk[..n]));
        }
    }
}

const THROWS: &str = "import { app } from \"bevy-ts\";\nthrow new Error(\"broken on open\");\n";
const FIXED: &str = r#"import { app, Startup } from "bevy-ts";
export function hello() { console.log("game started"); }
app.addSystems(Startup, hello);
"#;

fn reports(v: &Value) -> Vec<(String, u64)> {
    v["reports"].as_array().into_iter().flatten().map(|r| (r["status"].as_str().unwrap_or("").to_string(), r["generation"].as_u64().unwrap_or(0))).collect()
}

#[test]
fn an_engine_whose_entry_throws_stays_up_and_runs_the_game_after_the_fix() {
    let dir = project("viewport-preview-throws", &[("game.ts", THROWS)]);
    write_gltf(&dir.join("assets/models/grunt.gltf"));
    let port = 15990 + (std::process::id() % 9) as u16;
    let child = Command::new(env!("CARGO_BIN_EXE_bevy-ts"))
        .args(["dev", dir.join("game.ts").to_str().unwrap(), "--viewport", "--remote-port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _engine = Engine(child);
    let hello = until(port, "bevy-ts/hello", Value::Null, "the engine never answered hello", |_| true);
    assert_eq!(hello["viewport"], true);
    assert_eq!(hello["protocol"], bevy_ts_remote::protocol::VERSION);
    Stream::open(port, "bevy-ts/log+watch").until("the first load's error never reached the log", |v| v.to_string().contains("broken on open"));
    assert_eq!(post(port, "bevy-ts/hello", Value::Null).unwrap()["generation"], 0, "an empty world: nothing applied");
    let preview = until(port, "bevy-ts/asset_preview", json!({ "path": "models/grunt.gltf" }), "no preview with a failed game", |v| v["preview"].is_u64());
    assert_eq!(preview["kind"], "mesh");

    let mut reloads = Stream::open(port, "bevy-ts/reload+watch");
    // The stream's first message comes once it has a cursor; a save after that is reported.
    std::thread::sleep(Duration::from_millis(300));
    write(&dir, "game.ts", &THROWS.replace("broken on open", "still broken"));
    reloads.until("a second failure was not reported", |v| reports(v).iter().any(|(s, _)| s == "failed"));
    write(&dir, "game.ts", FIXED);
    let applied = reloads.until("the fix never applied", |v| reports(v).iter().any(|(s, _)| s == "applied"));
    assert!(reports(&applied).contains(&("applied".into(), 1)), "the fix applies as a first load: {applied}");
    Stream::open(port, "bevy-ts/log+watch").until("the fixed game never started", |v| v.to_string().contains("game started"));
}
