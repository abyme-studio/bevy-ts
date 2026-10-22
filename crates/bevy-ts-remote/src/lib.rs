//! The bevy-ts Remote Protocol: `bevy-ts/*` methods served next to stock `bevy_remote`.
//!
//! The contract is `protocol/bevy-ts.openrpc.json`; [`protocol`] is generated from it. Feature
//! crates add their own methods from the same file with [`add_method`] /
//! [`add_watching_method`], so a method is served only when its crate's plugin is added.

pub mod protocol;
pub mod editor;
mod entities;
mod values;

use bevy::prelude::*;
use bevy::remote::http::RemoteHttpPlugin;
use bevy::remote::{BrpError, BrpResult, RemoteMethodSystemId, RemoteMethods, RemotePlugin, error_codes};
use bevy_ts::generation::{self, ReloadLog};
use bevy_ts::log::ScriptLog;
use bevy_ts::runtime::{RuntimeInfo, ScriptClock, ScriptFrame};
use bevy_ts::systems::TsSystems;
use serde_json::{Value, json};
use std::collections::HashMap;

/// The contract this build was generated from.
pub const OPENRPC: &str = include_str!("../../../protocol/bevy-ts.openrpc.json");

/// The crate name core methods are tagged with in the contract.
pub const CRATE: &str = "bevy-ts-remote";

/// Serves the bevy-ts protocol (and stock BRP) over HTTP on `127.0.0.1:<port>`.
pub struct BevyTsRemotePlugin {
    pub port: u16,
}

impl Default for BevyTsRemotePlugin {
    fn default() -> Self {
        BevyTsRemotePlugin { port: bevy::remote::http::DEFAULT_PORT }
    }
}

impl Plugin for BevyTsRemotePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<RemotePlugin>() {
            app.add_plugins(RemotePlugin::default());
        }
        if !app.is_plugin_added::<RemoteHttpPlugin>() {
            app.add_plugins(RemoteHttpPlugin::default().with_port(self.port));
        }
        add_methods(app);
    }
}

/// Registers the core methods on an app that already has `RemotePlugin` (for a custom
/// transport, or for tests that call methods directly).
pub fn add_methods(app: &mut App) {
    add_method(app, CRATE, "bevy-ts/hello", hello);
    add_method(app, CRATE, "bevy-ts/discover", discover);
    add_method(app, CRATE, "bevy-ts/reload", reload);
    add_watching_method(app, CRATE, "bevy-ts/reload+watch", reload_watch);
    add_method(app, CRATE, "bevy-ts/schema", values::schema);
    add_method(app, CRATE, "bevy-ts/get_components", values::get_components);
    add_watching_method(app, CRATE, "bevy-ts/get_components+watch", values::get_components_watch);
    add_method(app, CRATE, "bevy-ts/mutate_components", values::mutate_components);
    add_method(app, CRATE, "bevy-ts/pause", pause);
    add_method(app, CRATE, "bevy-ts/step", step);
    add_method(app, CRATE, "bevy-ts/entities", entities::snapshot);
    add_watching_method(app, CRATE, "bevy-ts/entities+watch", entities::watch);
    add_method(app, CRATE, "bevy-ts/systems", systems);
    add_watching_method(app, CRATE, "bevy-ts/log+watch", log_watch);
    app.init_resource::<Subscribers>();
    app.add_systems(Last, entities::teardown_when_unwatched);
}

fn check_contract(crate_name: &str, name: &str, watch: bool) {
    let m = protocol::METHODS
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("{name} is not in protocol/bevy-ts.openrpc.json; add it there and regenerate"));
    assert_eq!(m.crate_name, crate_name, "{name} belongs to {} in the contract", m.crate_name);
    assert_eq!(m.watch, watch, "{name}: x-bevy-ts-watch disagrees with how it is registered");
}

/// Serves a method of the contract. Panics if the contract does not list it for `crate_name`.
pub fn add_method<M>(app: &mut App, crate_name: &str, name: &str, handler: impl IntoSystem<In<Option<Value>>, BrpResult, M> + 'static) {
    check_contract(crate_name, name, false);
    let id = app.world_mut().register_system(handler);
    app.world_mut().resource_mut::<RemoteMethods>().insert(name, RemoteMethodSystemId::Instant(id));
}

/// Serves a watching (`+watch`) method of the contract.
pub fn add_watching_method<M>(
    app: &mut App,
    crate_name: &str,
    name: &str,
    handler: impl IntoSystem<In<Option<Value>>, BrpResult<Option<Value>>, M> + 'static,
) {
    check_contract(crate_name, name, true);
    let id = app.world_mut().register_system(handler);
    app.world_mut().resource_mut::<RemoteMethods>().insert(name, RemoteMethodSystemId::Watching(id));
}

pub(crate) fn invalid(message: impl Into<String>) -> BrpError {
    BrpError { code: error_codes::INVALID_PARAMS, message: message.into(), data: None }
}

pub(crate) fn internal(message: impl Into<String>) -> BrpError {
    BrpError { code: error_codes::INTERNAL_ERROR, message: message.into(), data: None }
}

pub(crate) fn params<T: serde::de::DeserializeOwned + Default>(p: Option<Value>) -> Result<T, BrpError> {
    match p {
        None | Some(Value::Null) => Ok(T::default()),
        Some(v) => serde_json::from_value(v).map_err(|e| invalid(format!("invalid params: {e}"))),
    }
}

pub(crate) fn required<T: serde::de::DeserializeOwned>(p: Option<Value>) -> Result<T, BrpError> {
    serde_json::from_value(p.ok_or_else(|| invalid("params are required"))?).map_err(|e| invalid(format!("invalid params: {e}")))
}

/// bevy-ts's frame counter, for watching methods.
pub fn frame(world: &World) -> u64 {
    world.get_resource::<ScriptFrame>().map_or(0, |f| f.0)
}

/// Per-subscriber cursors for watching methods, keyed by method and params. Watching handlers
/// run once per open request per frame with that request's params; a key not served last frame
/// belongs to a closed request, so the next call with it is a new subscriber.
#[derive(Resource, Default)]
pub struct Subscribers {
    cursors: HashMap<(String, String), (u64, u64)>,
}

impl Subscribers {
    /// Returns the subscriber's cursor, or `None` for a new one, and marks it served this frame.
    pub fn cursor(&mut self, method: &str, params: &Option<Value>, frame: u64) -> Option<u64> {
        let key = (method.to_string(), params.as_ref().map(Value::to_string).unwrap_or_default());
        match self.cursors.get_mut(&key) {
            Some((last_frame, cursor)) if *last_frame + 1 >= frame => {
                *last_frame = frame;
                Some(*cursor)
            }
            _ => {
                self.cursors.insert(key, (frame, 0));
                None
            }
        }
    }

    pub fn set(&mut self, method: &str, params: &Option<Value>, cursor: u64) {
        let key = (method.to_string(), params.as_ref().map(Value::to_string).unwrap_or_default());
        if let Some(c) = self.cursors.get_mut(&key) {
            c.1 = cursor;
        }
    }
}

fn hello(In(_): In<Option<Value>>, world: &World) -> BrpResult {
    let info = world.resource::<RuntimeInfo>();
    let result = protocol::HelloResult {
        protocol: protocol::VERSION.into(),
        bevy_ts: env!("CARGO_PKG_VERSION").into(),
        bevy: "0.20".into(),
        engine: info.engine.clone(),
        entry: info.entry.as_ref().map(|p| p.display().to_string()),
        generation: info.generation,
        dev: protocol::DevFeatures {
            reload: world.contains_resource::<generation::DevSource>(),
            inspector: info.inspector.clone(),
        },
        viewport: Some(world.resource::<RemoteMethods>().get("bevy-ts/viewport_attach").is_some()),
        build: world.get_resource::<bevy_ts::engine::EngineIdentity>().map(|id| protocol::EngineBuild {
            kind: match id.kind {
                bevy_ts::engine::EngineKind::Prebuilt => protocol::EngineBuildKind::Prebuilt,
                bevy_ts::engine::EngineKind::Project => protocol::EngineBuildKind::Project,
            },
            hash: id.hash.clone(),
            native: id.native.as_ref().map(|n| protocol::NativeCrate { name: n.name.clone(), version: n.version.clone() }),
        }),
        sets: world
            .get_resource::<bevy_ts::sets::TsSets>()
            .map(|s| s.infos().into_iter().map(|i| protocol::ExposedSet { name: i.name, schedule: i.schedule }).collect())
            .unwrap_or_default(),
        params: world
            .get_resource::<bevy_ts::host::HostParams>()
            .map(|h| {
                h.specs()
                    .map(|p| protocol::ExportedParam {
                        name: p.ts.to_string(),
                        module: p.module.to_string(),
                        methods: p.methods.iter().map(|m| m.name.clone()).collect(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    serde_json::to_value(result).map_err(|e| internal(e.to_string()))
}

fn discover(In(_): In<Option<Value>>) -> BrpResult {
    serde_json::from_str(OPENRPC).map_err(|e| internal(e.to_string()))
}

fn reload(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::ReloadParams = params(p)?;
    let report = generation::reload_now(world, p.restart).map_err(|e| BrpError {
        code: error_codes::METHOD_NOT_FOUND,
        message: e.message,
        data: None,
    })?;
    serde_json::to_value(report).map_err(|e| internal(e.to_string()))
}

fn reload_watch(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let frame = frame(world);
    let next = world.resource::<ReloadLog>().next_seq();
    let mut subs = world.resource_mut::<Subscribers>();
    let Some(cursor) = subs.cursor("reload", &p, frame) else {
        subs.set("reload", &p, next);
        return Ok(None);
    };
    if cursor >= next {
        return Ok(None);
    }
    subs.set("reload", &p, next);
    let reports: Vec<_> = world.resource::<ReloadLog>().since(cursor).cloned().collect();
    Ok(Some(json!({ "reports": reports })))
}

fn log_watch(In(p): In<Option<Value>>, world: &mut World) -> BrpResult<Option<Value>> {
    let frame = frame(world);
    let next = world.resource::<ScriptLog>().next_seq();
    let cursor = world.resource_mut::<Subscribers>().cursor("log", &p, frame);
    // A new subscriber gets the recent history first.
    let from = cursor.unwrap_or(next.saturating_sub(100));
    if from >= next {
        return Ok(None);
    }
    world.resource_mut::<Subscribers>().set("log", &p, next);
    let entries: Vec<_> = world.resource::<ScriptLog>().since(from).cloned().collect();
    if entries.is_empty() {
        return Ok(None);
    }
    Ok(Some(json!({ "entries": entries })))
}

fn pause(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let p: protocol::PauseParams = required(p)?;
    // Resuming stages whose `Startup` never ran would run `Update` on an unstarted game.
    if !p.paused && !generation::started(world) {
        return Err(invalid("the game has not started; switch to play"));
    }
    let mut clock = world.resource_mut::<ScriptClock>();
    clock.paused = p.paused;
    if !p.paused {
        clock.step_frames = 0;
    }
    Ok(json!({ "paused": p.paused, "frame": frame(world) }))
}

fn step(In(p): In<Option<Value>>, world: &mut World) -> BrpResult {
    let frames = match p {
        None | Some(Value::Null) => 1,
        Some(v) => serde_json::from_value::<protocol::StepParams>(v).map_err(|e| invalid(format!("invalid params: {e}")))?.frames.get(),
    };
    if !world.resource::<ScriptClock>().paused {
        return Err(invalid("bevy-ts/step works while paused; call bevy-ts/pause first"));
    }
    // A step before the game started starts it, as Unity's Step from edit does.
    generation::start_game(world);
    world.resource_mut::<ScriptClock>().step_frames += frames;
    Ok(json!({ "paused": true, "frame": frame(world) }))
}

fn systems(In(_): In<Option<Value>>, world: &World) -> BrpResult {
    let list: Vec<_> = world.resource::<TsSystems>().iter().map(|s| s.info()).collect();
    serde_json::to_value(list).map_err(|e| internal(e.to_string()))
}
