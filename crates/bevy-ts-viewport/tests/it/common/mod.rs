//! Shared by the viewport tests: an app with the virtual window, protocol calls validated
//! against the contract, and an in-process viewer.
#![allow(dead_code)]

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts_viewport::ViewportPlugin;
use bevy_ts_viewport::mach::{Incoming, Viewer, release_port};
use bevy_ts_viewport::surface::Surface;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Held by the app that renders, until it drops. These tests once ran with `--test-threads=1`:
/// several render apps at once starve each other on a loaded machine. The lock keeps them one at a
/// time and lets the rest run in parallel, so the crate's tests run in the workspace's
/// `cargo test` and build no Bevy of their own (docs/build-cache.md).
pub struct OneRenderer(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

/// A new app holding the render lock. Every test app that renders starts here.
pub fn render_app() -> App {
    static RENDERER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let mut app = App::new();
    app.insert_non_send(OneRenderer(RENDERER.lock().unwrap_or_else(|e| e.into_inner())));
    app
}

pub fn app() -> App {
    app_with(|_| {})
}

/// An app with extra plugins added before it is finished.
pub fn app_with(extra: impl FnOnce(&mut App)) -> App {
    let mut app = render_app();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(RemotePlugin::default())
    .add_plugins(ViewportPlugin::default())
    .add_systems(Startup, scene);
    extra(&mut app);
    app.finish();
    app.cleanup();
    app
}

#[derive(Component)]
pub struct TheCube;

pub fn scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((DirectionalLight::default(), Transform::from_xyz(3.0, 6.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((TheCube, Mesh3d(meshes.add(Cuboid::from_size(Vec3::splat(2.0)))), MeshMaterial3d(materials.add(StandardMaterial::default()))));
}

fn rewrite(v: &mut Value) {
    match v {
        Value::Object(o) => {
            if let Some(Value::String(r)) = o.get_mut("$ref") {
                *r = r.replace("#/components/schemas/", "#/definitions/");
            }
            o.values_mut().for_each(rewrite);
        }
        Value::Array(a) => a.iter_mut().for_each(rewrite),
        _ => {}
    }
}

/// Validates a method's result against `protocol/bevy-ts.openrpc.json`.
pub fn validate(method: &str, result: &Value) {
    let doc: Value = serde_json::from_str(bevy_ts_remote::OPENRPC).unwrap();
    let m = doc["methods"].as_array().unwrap().iter().find(|m| m["name"] == method).unwrap();
    assert_eq!(m["x-bevy-ts-crate"], "bevy-ts-viewport");
    let mut schema = m["result"]["schema"].clone();
    let mut defs = doc["components"]["schemas"].clone();
    rewrite(&mut schema);
    rewrite(&mut defs);
    let mut root = json!({ "$schema": "http://json-schema.org/draft-07/schema#", "definitions": defs });
    if let Value::Object(s) = schema {
        root.as_object_mut().unwrap().extend(s);
    }
    let v = jsonschema::draft7::new(&root).unwrap();
    let errors: Vec<String> = v.iter_errors(result).map(|e| format!("{} at {}", e, e.instance_path)).collect();
    assert!(errors.is_empty(), "{method} answered off-contract: {errors:?}\n{result:#}");
}

pub fn call(app: &mut App, method: &str, params: Value) -> Result<Value, String> {
    let id = match app.world().resource::<RemoteMethods>().get(method) {
        Some(RemoteMethodSystemId::Instant(id)) => *id,
        _ => return Err(format!("no method {method}")),
    };
    let params = (!params.is_null()).then_some(params);
    let r = app.world_mut().run_system_with(id, params).unwrap().map_err(|e| e.message)?;
    validate(method, &r);
    Ok(r)
}

/// What a viewer has received.
#[derive(Default)]
pub struct Seen {
    pub surfaces: BTreeMap<(u64, u64), Surface>,
    pub frames: Vec<Value>,
    pub other: Vec<(String, Value)>,
}

pub fn pump(viewer: &Viewer, seen: &mut Seen, wait: Duration) {
    while let Some(m) = viewer.recv(wait) {
        let Incoming::Message { method, params, port } = m else { continue };
        match method.as_str() {
            "surface" => {
                let port = port.unwrap();
                let s = Surface::from_mach_port(port).unwrap();
                release_port(port);
                seen.surfaces.insert((params["ring"].as_u64().unwrap(), params["slot"].as_u64().unwrap()), s);
            }
            "frame" => seen.frames.push(params),
            _ => seen.other.push((method, params)),
        }
    }
}

/// Updates until `done` or the timeout (pipelines compile on the first frames).
pub fn run_until(app: &mut App, viewer: &Viewer, seen: &mut Seen, timeout: Duration, mut done: impl FnMut(&Seen) -> bool) {
    let t0 = Instant::now();
    while !done(seen) {
        assert!(t0.elapsed() < timeout, "timed out");
        app.update();
        pump(viewer, seen, Duration::from_millis(2));
    }
}

pub fn attach(app: &mut App, w: f64, h: f64, scale: f64) -> (String, Viewer) {
    let r = call(app, "bevy-ts/viewport_attach", json!({ "w": w, "h": h, "scale": scale, "hz": 120 })).unwrap();
    let service = r["service"].as_str().unwrap().to_string();
    assert!(service.starts_with(&format!("dev.bevy-ts.viewport.{}.", std::process::id())));
    assert_eq!(r["protocol"], bevy_ts_remote::protocol::VERSION);
    let viewer = Viewer::connect(&service).unwrap();
    (service, viewer)
}

pub const LONG: Duration = Duration::from_secs(60);

/// A one-mesh glTF, a unit cube under one node, buffer inline, with a translation animation per
/// name in `animations`.
pub fn write_gltf(path: &std::path::Path, animations: &[&str]) {
    let p: [[f32; 3]; 8] = [[-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, 0.5, -0.5], [-0.5, 0.5, -0.5], [-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.5, 0.5, 0.5], [-0.5, 0.5, 0.5]];
    let idx: [u16; 36] = [0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7, 3, 1, 2, 6, 1, 6, 5];
    let mut buf = vec![];
    for c in p.iter().flatten() {
        buf.extend_from_slice(&c.to_le_bytes());
    }
    for i in idx {
        buf.extend_from_slice(&i.to_le_bytes());
    }
    // Keyframe times, then translations: 0 to 1 m up over a second.
    for t in [0.0f32, 1.0] {
        buf.extend_from_slice(&t.to_le_bytes());
    }
    for c in [0.0f32, 0.0, 0.0, 0.0, 1.0, 0.0] {
        buf.extend_from_slice(&c.to_le_bytes());
    }
    let uri = format!("data:application/octet-stream;base64,{}", bevy_ts_viewport::assets::base64(&buf));
    let anims: Vec<Value> = animations
        .iter()
        .map(|n| json!({ "name": n, "channels": [{ "sampler": 0, "target": { "node": 1, "path": "translation" } }], "samplers": [{ "input": 2, "output": 3 }] }))
        .collect();
    let mut doc = json!({
        "asset": { "version": "2.0" },
        "scene": 0, "scenes": [{ "nodes": [0] }], "nodes": [{ "name": "root", "children": [1] }, { "name": "cube", "mesh": 0 }],
        "meshes": [{ "primitives": [{ "attributes": { "POSITION": 0 }, "indices": 1 }] }],
        "buffers": [{ "byteLength": buf.len(), "uri": uri }],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": 96 },
            { "buffer": 0, "byteOffset": 96, "byteLength": 72 },
            { "buffer": 0, "byteOffset": 168, "byteLength": 8 },
            { "buffer": 0, "byteOffset": 176, "byteLength": 24 }
        ],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 8, "type": "VEC3", "min": [-0.5, -0.5, -0.5], "max": [0.5, 0.5, 0.5] },
            { "bufferView": 1, "componentType": 5123, "count": 36, "type": "SCALAR" },
            { "bufferView": 2, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0.0], "max": [1.0] },
            { "bufferView": 3, "componentType": 5126, "count": 2, "type": "VEC3" }
        ]
    });
    if !anims.is_empty() {
        doc["animations"] = json!(anims);
    }
    std::fs::write(path, doc.to_string()).unwrap();
}

/// A fresh asset root under the temp dir with `models/`.
pub fn asset_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("bevy-ts-viewport-preview").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("models")).unwrap();
    dir
}

/// The test app over an asset root.
pub fn app_at(dir: &std::path::Path) -> App {
    let mut app = render_app();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: dir.display().to_string(), ..default() }),
    )
    .add_plugins(RemotePlugin::default())
    .add_plugins(ViewportPlugin::default())
    .insert_resource(bevy_ts_viewport::assets::AssetRoot(dir.to_path_buf()))
    .add_systems(Startup, scene);
    app.finish();
    app.cleanup();
    app.update();
    app
}

/// Asks `asset_preview` until it answers a preview.
pub fn open_preview(app: &mut App, path: &str) -> Value {
    let t0 = Instant::now();
    loop {
        let r = call(app, "bevy-ts/asset_preview", json!({ "path": path })).unwrap();
        if r["pending"] == false {
            return r;
        }
        assert!(t0.elapsed() < LONG, "{path} never loaded");
        std::thread::sleep(Duration::from_millis(2));
        app.update();
    }
}

/// Attaches a viewer to `view`; answers the viewport id and the viewer.
pub fn attach_view(app: &mut App, w: f64, h: f64, view: Value) -> (u64, Viewer) {
    let r = call(app, "bevy-ts/viewport_attach", json!({ "w": w, "h": h, "scale": 1, "hz": 120, "view": view })).unwrap();
    let viewer = Viewer::connect(r["service"].as_str().unwrap()).unwrap();
    (r["viewport"].as_u64().unwrap(), viewer)
}
