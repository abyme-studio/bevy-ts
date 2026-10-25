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
    if !app.world().contains_resource::<Subscribers>() {
        app.init_resource::<Subscribers>().add_systems(First, next_update);
    }
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

/// Per-subscriber cursors for watching methods, keyed by method and params.
///
/// Bevy runs a watching handler once per open request per update, with that request's params and
/// nothing else, in the order of one list of every open watch. A new request is pushed on its end,
/// and a closed one is dropped with `swap_remove`, which moves the last request into its place. So
/// requests with the same params are told apart by their order within the update: the first `n`
/// calls of an update, where `n` subscribers were served the update before, are those
/// subscribers, and a call after them is a new one. A subscriber not served for an update has
/// closed.
///
/// After a close the survivors can swap slots, and one can take another's cursor. A slot is
/// therefore served the lowest cursor among it and the slots after it: a subscriber can be sent a
/// message again, never miss one. Streams that send everything since the cursor catch every
/// subscriber up each update, so their cursors are equal and a swap changes nothing; `level+watch`,
/// which sends one event per update, is the exception and can repeat events after a close. One
/// that opens in the update another with the same params closes is taken for the old one, and
/// gets no opening message.
#[derive(Resource, Default)]
pub struct Subscribers {
    update: u64,
    streams: HashMap<(String, String), Stream>,
}

/// The subscribers of one method with one set of params.
#[derive(Default)]
struct Stream {
    /// The update these subscribers were last served in.
    update: u64,
    /// How many were served in the update before that one.
    before: usize,
    /// How many have been served in `update`.
    served: usize,
    /// What each was last sent, in serving order.
    cursors: Vec<u64>,
}

/// One call of a watching method: which subscriber it serves, and its cursor (`None` when the
/// subscriber is new).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Turn {
    pub slot: usize,
    pub cursor: Option<u64>,
}

impl Subscribers {
    /// Marks the subscriber this call serves as served, and says which it is.
    pub fn turn(&mut self, method: &str, params: &Option<Value>) -> Turn {
        let now = self.update;
        let s = self.streams.entry(key(method, params)).or_default();
        if s.update != now {
            s.before = if s.update + 1 == now { s.served } else { 0 };
            s.cursors.truncate(s.before);
            s.served = 0;
            s.update = now;
        }
        let slot = s.served;
        s.served += 1;
        if slot < s.before {
            // Any later slot's subscriber may have been moved here (see above).
            let cursor = s.cursors[slot..s.before].iter().copied().min();
            return Turn { slot, cursor };
        }
        s.cursors.truncate(slot);
        s.cursors.push(0);
        Turn { slot, cursor: None }
    }

    /// The update count, advanced in `First`. Unlike `ScriptFrame`, a Stop does not reset it.
    pub fn update(&self) -> u64 {
        self.update
    }

    /// Returns the subscriber's cursor, or `None` for a new one, and marks it served.
    pub fn cursor(&mut self, method: &str, params: &Option<Value>) -> Option<u64> {
        self.turn(method, params).cursor
    }

    /// Sets the cursor of the subscriber the last [`Subscribers::turn`] served.
    pub fn set(&mut self, method: &str, params: &Option<Value>, cursor: u64) {
        if let Some(s) = self.streams.get_mut(&key(method, params))
            && let Some(c) = s.served.checked_sub(1).and_then(|i| s.cursors.get_mut(i))
        {
            *c = cursor;
        }
    }

    /// Whether a counter at `seq` is news to this subscriber: it is new, or `seq` moved since it
    /// was last sent. For streams that send their whole state on open and on every change.
    pub fn owed(&mut self, method: &str, params: &Option<Value>, seq: u64) -> bool {
        let sent = self.cursor(method, params);
        self.set(method, params, seq);
        sent != Some(seq)
    }
}

fn key(method: &str, params: &Option<Value>) -> (String, String) {
    (method.to_string(), params.as_ref().map(Value::to_string).unwrap_or_default())
}

/// `First`: a new update for [`Subscribers`].
fn next_update(mut s: ResMut<Subscribers>) {
    s.update += 1;
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
                        stands_for: p.stands_for.map(str::to_string),
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
    let next = world.resource::<ReloadLog>().next_seq();
    let mut subs = world.resource_mut::<Subscribers>();
    let Some(cursor) = subs.cursor("reload", &p) else {
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
    let next = world.resource::<ScriptLog>().next_seq();
    let cursor = world.resource_mut::<Subscribers>().cursor("log", &p);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Two subscribers with the same params, one joining two updates late, then the first closing.
    #[test]
    fn subscribers_with_the_same_params_each_get_their_own_cursor() {
        let mut s = Subscribers::default();
        let p = Some(json!({ "k": 1 }));
        let a = s.turn("m", &p);
        assert_eq!(a, Turn { slot: 0, cursor: None }, "a is new");
        s.set("m", &p, 5);
        s.update += 1;
        assert_eq!(s.cursor("m", &p), Some(5));
        s.update += 1;
        assert_eq!(s.cursor("m", &p), Some(5), "a, served first");
        assert_eq!(s.turn("m", &p), Turn { slot: 1, cursor: None }, "b joins: new, though a has these params");
        s.set("m", &p, 9);
        s.update += 1;
        assert_eq!(s.cursor("m", &p), Some(5), "a keeps its cursor");
        s.set("m", &p, 7);
        assert_eq!(s.cursor("m", &p), Some(9), "b has its own");
        s.set("m", &p, 12);
        // a closes: b moves into a's slot, and is served from the lower of the two cursors.
        s.update += 1;
        assert_eq!(s.cursor("m", &p), Some(7), "sent again, never skipped");
        s.set("m", &p, 12);
        s.update += 1;
        assert_eq!(s.cursor("m", &p), Some(12));
        // Nobody served for an update: the next call is a new subscriber.
        s.update += 2;
        assert_eq!(s.cursor("m", &p), None);
    }

    /// Three subscribers at different cursors; the first closes and Bevy moves the last into its
    /// place. Nobody is served past what it was sent.
    #[test]
    fn a_close_never_skips_a_subscriber_past_its_cursor() {
        let mut s = Subscribers::default();
        let p = None;
        for c in [3, 8, 5] {
            s.cursor("m", &p);
            s.set("m", &p, c);
        }
        s.update += 1;
        // Served in the order [c, b]: c (sent 5) sits in a's slot, b in its own.
        let first = s.cursor("m", &p).unwrap();
        assert!(first <= 5, "c is served from at most what it was sent: {first}");
        s.set("m", &p, 9);
        let second = s.cursor("m", &p).unwrap();
        assert!(second <= 8, "b likewise: {second}");
        s.set("m", &p, 9);
        s.update += 1;
        assert_eq!((s.cursor("m", &p), s.cursor("m", &p)), (Some(9), Some(9)));
    }

    #[test]
    fn owed_sends_on_open_and_when_the_counter_moves() {
        let mut s = Subscribers::default();
        let p = None;
        assert!(s.owed("m", &p, 3), "on open");
        s.update += 1;
        assert!(!s.owed("m", &p, 3));
        assert!(s.owed("m", &p, 3), "a second subscriber opens");
        s.update += 1;
        assert!(s.owed("m", &p, 4));
        assert!(s.owed("m", &p, 4), "both are told");
        s.update += 1;
        assert!(!s.owed("m", &p, 4));
        assert!(!s.owed("m", &p, 4));
    }
}
