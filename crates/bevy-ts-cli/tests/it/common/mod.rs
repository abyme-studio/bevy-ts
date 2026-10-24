#![allow(dead_code)]

use bevy::prelude::*;
use bevy_ts::generation::{DevSource, ReloadReport};
use bevy_ts::log::{LogEntry, ScriptLog};
use bevy_ts::BevyTsPlugin;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_v8::V8Options;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// The `bevy-ts` binary under test.
pub const BIN: &str = env!("CARGO_BIN_EXE_bevy-ts");

/// A free local port for an engine's remote server. Every module runs in this one process, so a
/// port taken from the process id would be the same for all of them.
/// Pipelines compiled on the render thread, as Bevy's own pixel tests do: a test that reads pixels
/// a set number of frames in would otherwise see a mesh draw some frames late, since an engine
/// compiles pipelines in the background (`vendor/bevy_render/PATCH.md`).
pub fn sync_pipelines() -> bevy::render::RenderPlugin {
    bevy::render::RenderPlugin { synchronous_pipeline_compilation: true, ..Default::default() }
}

pub fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port()
}

/// A fresh directory of TS files under the temp dir.
pub fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    // Per process: another worktree's run of the same test must not share, or delete, its files.
    let dir = std::env::temp_dir().join("bevy-ts-cli-tests").join(format!("{name}-{}", std::process::id()));
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

/// An engine (or any long-lived child) a test started. It stays in the test runner's process
/// group, so Ctrl-C or a hangup to `cargo test` reaches it as it reaches the runner. Dropping it
/// kills it and every process under it and reaps it, so a test that panics leaves nothing behind,
/// a supervised engine's child included.
pub struct Engine(Child);

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn getpgid(pid: i32) -> i32;
}

impl Engine {
    pub fn spawn(cmd: &mut Command) -> Engine {
        Engine(cmd.spawn().unwrap())
    }

    pub fn id(&self) -> u32 {
        self.0.id()
    }

    pub fn child(&mut self) -> &mut Child {
        &mut self.0
    }

    /// Every process under `pid`, children before their own.
    pub fn descendants(pid: u32) -> Vec<u32> {
        let out = Command::new("pgrep").args(["-P", &pid.to_string()]).output().map(|o| o.stdout).unwrap_or_default();
        let children: Vec<u32> = String::from_utf8_lossy(&out).lines().filter_map(|l| l.trim().parse().ok()).collect();
        children.iter().flat_map(|&c| std::iter::once(c).chain(Engine::descendants(c))).collect()
    }

    /// Whether `pid` is still a process (a zombie not yet reaped counts).
    #[cfg(unix)]
    pub fn alive(pid: u32) -> bool {
        unsafe { kill(pid as i32, 0) == 0 }
    }

    /// The process group `pid` is in.
    #[cfg(unix)]
    pub fn group(pid: u32) -> i32 {
        unsafe { getpgid(pid as i32) }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // The tree first, while it is still under the engine; then the engine, so a supervisor
        // cannot start another child; then what was under it, now orphaned.
        let under = Engine::descendants(self.0.id());
        let _ = self.0.kill();
        #[cfg(unix)]
        for pid in under {
            unsafe {
                kill(pid as i32, 9);
            }
        }
        let _ = self.0.wait();
    }
}

/// How long an engine may take from spawn to its first applied generation. A first launch of a
/// fresh binary on macOS under heavy load can stall for over a minute while the system scans it
/// (the research team's first-launch-scan), so the launch gets this long bound of its own; what
/// a test waits for after that keeps a short one, so real failures still show there.
pub const LAUNCH: Duration = Duration::from_secs(300);

/// The outer bound on waiting for what an engine does off the test's thread: a compile, an asset
/// load, a file watcher, a launch's later steps. A loaded machine slows those by any amount, so a
/// test waits on the event itself and this bound only catches what never comes.
pub const BOUND: Duration = Duration::from_secs(120);

/// Updates `app` until `done` holds, and returns how long that took. Past `BOUND` it panics
/// naming `what`, with the script log: what did arrive.
#[track_caller]
pub fn until(app: &mut App, what: &str, mut done: impl FnMut(&mut App) -> bool) -> Duration {
    let start = Instant::now();
    loop {
        app.update();
        if done(app) {
            return start.elapsed();
        }
        assert!(start.elapsed() < BOUND, "{what}: not within {BOUND:?}; the log: {:?}", messages(app));
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Updates until every level root has loaded and reconciled.
#[track_caller]
pub fn settle_levels(app: &mut App) {
    until(app, "the level never loaded", |app| {
        let levels = app.world().resource::<bevy_ts_levels::engine::Levels>();
        !levels.roots.is_empty() && levels.roots.values().all(|r| !r.dirty && !r.applied.is_empty())
    });
}

/// Spawns `bevy-ts play <folder> --headless` with its stderr lines on a channel.
pub fn play(folder: &Path) -> (Engine, Receiver<String>) {
    let mut engine = Engine::spawn(
        Command::new(BIN)
            .args(["play", folder.to_str().unwrap(), "--headless"])
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
    (engine, rx)
}

/// Lines until every needle has been seen, or the deadline.
pub fn wait_for(rx: &Receiver<String>, needles: &[&str], within: Duration) -> Vec<String> {
    let mut seen = Vec::new();
    wait_more(rx, &mut seen, needles, within);
    seen
}

/// As `wait_for`, adding to lines already seen, which count toward the needles.
pub fn wait_more(rx: &Receiver<String>, seen: &mut Vec<String>, needles: &[&str], within: Duration) {
    let deadline = Instant::now() + within;
    while !needles.iter().all(|n| seen.iter().any(|l: &String| l.contains(n))) {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(l) => seen.push(l),
            Err(_) => panic!("never saw {needles:?} within {within:?}; got:\n{}", seen.join("\n")),
        }
    }
}

/// The TCP ports a process listens on, from `lsof`.
pub fn listening(pid: u32) -> Vec<String> {
    let out = Command::new("lsof").args(["-a", "-p", &pid.to_string(), "-iTCP", "-sTCP:LISTEN", "-Fn"]).output().expect("lsof");
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.strip_prefix('n').map(str::to_string)).collect()
}
