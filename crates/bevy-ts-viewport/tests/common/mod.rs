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

pub fn app() -> App {
    app_with(|_| {})
}

/// An app with extra plugins added before it is finished.
pub fn app_with(extra: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
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
