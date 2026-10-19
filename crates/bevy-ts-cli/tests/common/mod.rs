#![allow(dead_code)]

use bevy::prelude::*;
use bevy_ts::generation::{DevSource, ReloadReport};
use bevy_ts::log::{LogEntry, ScriptLog};
use bevy_ts::BevyTsPlugin;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_v8::V8Options;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A fresh directory of TS files under the temp dir.
pub fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join("bevy-ts-cli-tests").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (f, src) in files {
        write(&dir, f, src);
    }
    dir
}

pub fn write(dir: &Path, file: &str, src: &str) {
    let p = dir.join(file);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, src).unwrap();
}

/// A headless app with the V8 backend whose dev source compiles `entry`.
pub fn app(entry: &Path) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true));
    app.add_plugins(bevy_ts_tsx::TsxPlugin);
    app.insert_resource(DevSource(Arc::new(Compiler::new(entry, true).unwrap())));
    app
}

pub fn reload(app: &mut App, restart: bool) -> ReloadReport {
    bevy_ts::generation::reload_now(app.world_mut(), restart).unwrap()
}

pub fn logs(app: &App) -> Vec<LogEntry> {
    app.world().resource::<ScriptLog>().since(0).cloned().collect()
}

pub fn messages(app: &App) -> Vec<String> {
    logs(app).into_iter().map(|e| e.message).collect()
}

pub fn updates(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update();
    }
}

/// As `app`, with the V8 inspector on an ephemeral port.
pub fn app_with_inspector(entry: &Path) -> App {
    let mut app = App::new();
    let options = V8Options { inspect: Some(std::net::SocketAddr::from(([127, 0, 0, 1], 0))) };
    app.add_plugins(MinimalPlugins).add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(options)).dev(true));
    app.insert_resource(DevSource(Arc::new(Compiler::new(entry, true).unwrap())));
    app
}

/// A one-module project loaded as generation 1; panics with the errors if it fails.
pub fn load(name: &str, src: &str) -> (PathBuf, App) {
    let dir = project(name, &[("game.ts", src)]);
    let mut app = app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "load failed: {:?}", r.errors);
    (dir, app)
}

/// The errors a one-module project fails to load with.
pub fn load_errors(name: &str, src: &str) -> Vec<bevy_ts::backend::ScriptError> {
    let dir = project(name, &[("game.ts", src)]);
    let mut app = app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, bevy_ts::generation::ReloadStatus::Failed, "expected a failed load");
    r.errors
}

/// Spawns an entity with a TS component (field values as JSON) and optional Rust bundle.
pub fn spawn_ts(app: &mut App, component: &str, values: serde_json::Value, extra: impl Bundle) -> Entity {
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), component).expect("live component");
    let bytes = bevy_ts::plugin_api::encode_component(app.world(), component, values.as_object().unwrap()).unwrap();
    let mut e = app.world_mut().spawn(extra);
    unsafe { bevy_ts::commands::insert_bytes(&mut e, id, &bytes) };
    e.id()
}

/// Adds a TS component to an existing entity.
pub fn insert_ts(app: &mut App, e: Entity, component: &str, values: serde_json::Value) {
    let (id, _) = bevy_ts::plugin_api::component_by_name(app.world(), component).expect("live component");
    let bytes = bevy_ts::plugin_api::encode_component(app.world(), component, values.as_object().unwrap()).unwrap();
    let mut em = app.world_mut().entity_mut(e);
    unsafe { bevy_ts::commands::insert_bytes(&mut em, id, &bytes) };
}

/// A TS component's value on an entity, as JSON.
pub fn read_ts(app: &App, e: Entity, component: &str) -> Option<serde_json::Value> {
    let c = app.world().resource::<bevy_ts::components::TsComponents>().get(component)?.clone();
    let p = app.world().get_by_id(e, c.id)?;
    let bytes = unsafe { std::slice::from_raw_parts(p.as_ptr(), c.layout.size) };
    Some(bevy_ts::value::decode_component(&c.layout, bytes))
}

/// A TS resource's value, as JSON.
pub fn read_res(app: &App, name: &str) -> Option<serde_json::Value> {
    let c = app.world().resource::<bevy_ts::components::TsComponents>().get(name)?.clone();
    let e = app.world().resource_entities().get(c.id)?;
    read_ts(app, e, name)
}

/// Entities whose TS component changed after `since`, through a Rust `QueryBuilder` query.
pub fn changed_since(app: &mut App, component: &str, since: u32) -> Vec<Entity> {
    let id = bevy_ts::plugin_api::component_by_name(app.world(), component).unwrap().0;
    let world = app.world_mut();
    let this_run = world.read_change_tick();
    let mut q = QueryBuilder::<(Entity, bevy::ecs::world::FilteredEntityRef)>::new(world).ref_id(id).build();
    let mut out: Vec<Entity> = q
        .iter(world)
        .filter(|(_, r)| r.get_change_ticks_by_id(id).unwrap().is_changed(bevy::ecs::change_detection::Tick::new(since), this_run))
        .map(|(e, _)| e)
        .collect();
    out.sort();
    out
}

/// Calls made into JS so far.
pub fn calls(app: &App) -> u64 {
    app.world().non_send::<bevy_ts::runtime::ScriptRuntime>().backend.calls_into_js()
}

pub fn tick(app: &mut App) -> u32 {
    app.world_mut().read_change_tick().get()
}
