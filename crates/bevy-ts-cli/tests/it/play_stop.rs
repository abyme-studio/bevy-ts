//! Change `play-stop`: Stop rebuilds the edit world from source. A game on V8 with TSX and a
//! level, in the app `bevy-ts dev --viewport` builds, played and stopped through the protocol.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::commands::TsSpawned;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_levels::engine::{Levels, LevelsPlugin, Opener};
use bevy_ts_v8::V8Options;
use common::{project, read_res, reload, until, write};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const GAME: &str = r##"import { app, Component, Resource, SceneComponent, type Commands, type Res, type ResMut, Startup, Time, Update, type f32 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";

export class Bullet extends Component {}
export class Score extends Resource { static path = "game::Score"; declare value: f32 }
export class StartedAt extends Resource { static path = "game::StartedAt"; declare secs: f32 }

export class Enemy extends SceneComponent {
  static scene() {
    return (
      <entity name="enemy">
        <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
        {/*CHILD*/}
      </entity>
    );
  }
}

export function fire(commands: Commands, at: ResMut<StartedAt>, time: Res<Time>) {
  at.secs = time.elapsed_secs;
  commands.spawn(Bullet, Transform.of({ translation: [0, 1, 0] }));
}
export function scoreUp(score: ResMut<Score>) { score.value += 1; }

app.insertResource(Score.of({ value: 0 })).insertResource(StartedAt.of({ secs: -1 }));
app.level("levels/arena.bsn");
app.addSystems(Startup, fire).addSystems(Update, scoreUp);
"##;

const ARENA: &str = "#EnemyA
@game::Enemy
Transform { translation: Vec3 { x: -3.0, z: -4.0 } }
--
#EnemyB
@game::Enemy
Transform { translation: Vec3 { x: 3.0, z: -4.0 } }
";

const ROOM: &str = "#Guard
@game::Enemy
Transform { translation: Vec3 { z: -16.0 } }
";

fn arena(name: &str) -> std::path::PathBuf {
    project(name, &[("src/game.tsx", GAME), ("assets/levels/arena.bsn", ARENA), ("assets/levels/room.bsn", ROOM)])
}

fn stop_app(dir: &Path) -> App {
    stop_app_booted(dir, true)
}

fn stop_app_booted(dir: &Path, edit: bool) -> App {
    stop_app_entry(dir, "src/game.tsx", edit)
}

/// Booted as `bevy-ts dev --viewport` boots, with `--edit` or without.
fn stop_app_entry(dir: &Path, entry: &str, edit: bool) -> App {
    // Canonical: the file watcher strips this root from the paths macOS reports (`/private/var`).
    let assets = dir.join("assets").canonicalize().unwrap();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: assets.to_string_lossy().into_owned(), ..default() }),
    )
    .add_plugins(RemotePlugin::default())
    .add_plugins(bevy_ts_viewport::ViewportPlugin { windowed: false })
    .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin)
    .add_plugins(LevelsPlugin);
    app.world_mut().resource_mut::<Levels>().asset_root = assets;
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join(entry), true).unwrap())));
    bevy_ts_remote::add_methods(&mut app);
    bevy_ts_levels::protocol::add_methods(&mut app);
    if edit {
        app.insert_state(bevy_ts::EngineMode::Edit);
        app.world_mut().resource_mut::<bevy_ts::generation::Generations>().started = false;
        app.world_mut().resource_mut::<bevy_ts::runtime::ScriptClock>().paused = true;
    }
    app.finish();
    app.cleanup();
    app
}

fn call(app: &mut App, method: &str, params: Value) -> Value {
    let id = match app.world().resource::<RemoteMethods>().get(method) {
        Some(RemoteMethodSystemId::Instant(id)) => *id,
        _ => panic!("{method} not served"),
    };
    app.world_mut().run_system_with(id, (!params.is_null()).then_some(params)).unwrap().unwrap_or_else(|e| panic!("{method}: {}", e.message))
}

fn updates(app: &mut App, n: usize) {
    for _ in 0..n {
        std::thread::sleep(Duration::from_millis(5));
        app.update();
    }
}

/// Entities by name, with their translation.
fn named(app: &mut App, name: &str) -> Vec<(Entity, Vec3)> {
    let world = app.world_mut();
    let mut q = world.query::<(Entity, &Name, &Transform)>();
    let mut out: Vec<(Entity, Vec3)> = q.iter(world).filter(|(_, n, _)| n.as_str() == name).map(|(e, _, t)| (e, t.translation)).collect();
    out.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
    out
}

/// Plays a freshly loaded arena: generation applied, its level standing, a few frames run.
fn played(name: &str) -> (std::path::PathBuf, App) {
    let dir = arena(name);
    let mut app = stop_app(&dir);
    app.update();
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    until(&mut app, "the level never loaded", |app| named(app, "EnemyA").len() == 1 && named(app, "EnemyB").len() == 1);
    assert_eq!(mode(&mut app, "play")["started"], true);
    updates(&mut app, 20);
    (dir, app)
}

fn mode(app: &mut App, mode: &str) -> Value {
    call(app, "bevy-ts/viewport_mode", json!({ "mode": mode }))
}

fn bullets(app: &mut App) -> usize {
    app.world_mut().query::<&TsSpawned>().iter(app.world()).count()
}

#[test]
fn stopping_the_arena_puts_it_back_as_the_source_describes_it() {
    let (_dir, mut app) = played("play-stop-arena");
    assert_eq!(bullets(&mut app), 1, "Startup fired once");
    assert!(read_res(&app, "game::Score").unwrap()["value"].as_f64().unwrap() > 3.0);
    // An enemy is destroyed, B is selected, and a client opens the room.
    let (a, _) = named(&mut app, "EnemyA")[0];
    app.world_mut().entity_mut(a).despawn();
    let (b, b_at) = named(&mut app, "EnemyB")[0];
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [b.to_bits()] }));
    let r = call(&mut app, "bevy-ts/level_open", json!({ "file": "levels/room.bsn" }));
    assert_eq!(r["status"], "opened");
    assert_eq!(named(&mut app, "Guard").len(), 1);
    std::thread::sleep(Duration::from_millis(200));
    updates(&mut app, 5);
    assert!(app.world().resource::<Time<Virtual>>().elapsed_secs() > 0.2);
    let events = app.world().resource::<Levels>().seq;

    let r = mode(&mut app, "edit");
    assert_eq!((r["mode"].as_str(), r["started"].as_bool()), (Some("edit"), Some(false)));
    // Answered after the rebuild and the levels' reconcile.
    assert_eq!(named(&mut app, "EnemyA").len(), 1, "the destroyed enemy is back");
    assert_eq!(named(&mut app, "EnemyA")[0].1, Vec3::new(-3.0, 0.0, -4.0));
    let (b2, b2_at) = named(&mut app, "EnemyB")[0];
    assert_eq!(b2_at, b_at);
    assert_eq!(read_res(&app, "game::Score").unwrap()["value"], 0.0, "Score at its insertResource value");
    assert_eq!(bullets(&mut app), 0, "what TS spawned is gone");
    // Only the arena, with its opener; the room's close was streamed.
    let levels = app.world().resource::<Levels>();
    assert_eq!(levels.roots.keys().cloned().collect::<Vec<_>>(), vec!["levels/arena.bsn".to_string()]);
    assert_eq!(levels.roots["levels/arena.bsn"].opener, Opener::Ts);
    assert!(levels.events.iter().any(|(seq, m)| *seq > events && m["event"] == "closed" && m["level"] == "levels/room.bsn"));
    assert!(named(&mut app, "Guard").is_empty());
    // Enemy B stays selected, with its new entity, streamed as the engine's.
    let sel = app.world().resource::<bevy_ts_viewport::edit::Selection>();
    assert_eq!(sel.items.iter().map(|(e, _)| *e).collect::<Vec<_>>(), vec![b2]);
    assert_eq!(sel.origin, bevy_ts_viewport::edit::Origin::Engine);
    updates(&mut app, 5);
    assert_eq!(read_res(&app, "game::Score").unwrap()["value"], 0.0, "nothing runs in edit");

    // Play again: Startup runs once more, at time 0.
    let r = mode(&mut app, "play");
    assert_eq!(r["started"], true);
    assert_eq!(bullets(&mut app), 1);
    assert_eq!(read_res(&app, "game::StartedAt").unwrap()["secs"], 0.0);
}

#[test]
fn a_save_during_play_is_what_stop_rebuilds_without_compiling() {
    let (dir, mut app) = played("play-stop-save");
    assert!(named(&mut app, "horn").is_empty());
    write(&dir, "src/game.tsx", &GAME.replace("{/*CHILD*/}", r##"<entity name="horn"><mesh shape="sphere" radius={0.2} color="#fff" /></entity>"##));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    updates(&mut app, 5);
    // A typo saved since: a Stop that compiled would fail on it.
    write(&dir, "src/game.tsx", "export const broken = ;\n");
    let r = mode(&mut app, "edit");
    assert_eq!(r["started"], false);
    assert_eq!(named(&mut app, "horn").len(), 2, "each enemy has the new child");
    let last = app.world().resource::<bevy_ts::generation::ReloadLog>().reports.last().unwrap().clone();
    assert_eq!(last.status, ReloadStatus::Applied, "the Stop re-applied the live set");
}

fn tree_watch(app: &mut App) -> Option<Value> {
    let id = match app.world().resource::<RemoteMethods>().get("bevy-ts/tsx_tree+watch") {
        Some(RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("bevy-ts/tsx_tree+watch not served"),
    };
    app.world_mut().run_system_with(id, None).unwrap().unwrap()
}

#[test]
fn the_tree_watch_follows_a_stop_to_the_new_entities() {
    let (_dir, mut app) = played("play-stop-watch");
    // A client holds the tree by node id, with each row's entity.
    let first = tree_watch(&mut app).unwrap();
    let mut client: std::collections::HashMap<String, u64> = first["snapshot"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["nodes"].as_array().unwrap().iter())
        .filter_map(|n| Some((n["node"].as_str()?.to_string(), n["entity"].as_u64()?)))
        .collect();
    let (old_a, _) = named(&mut app, "EnemyA")[0];
    assert!(client.values().any(|e| *e == old_a.to_bits()));
    // The play moves A, so the Stop respawns it, whether it keeps the arena or remounts it.
    app.world_mut().get_mut::<Transform>(old_a).unwrap().translation.y = 1.0;

    mode(&mut app, "edit");
    let mut seen = 0;
    // One call per update, as Bevy serves an open request.
    for _ in 0..5 {
        updates(&mut app, 1);
        if let Some(m) = tree_watch(&mut app) {
            assert!(m.get("snapshot").is_none(), "the watch resubscribed: {m}");
            for id in m["removed"].as_array().into_iter().flatten() {
                client.remove(id.as_str().unwrap());
            }
            for row in m["added"].as_array().into_iter().flatten() {
                seen += 1;
                client.insert(row["node"].as_str().unwrap().to_string(), row["entity"].as_u64().unwrap());
            }
        }
    }
    assert!(seen > 0, "the respawn was sent");
    // Every entity tsx made is a row of the client's tree, so none shows as spawned at runtime.
    let made: std::collections::BTreeSet<u64> =
        app.world_mut().query_filtered::<Entity, With<bevy_ts_tsx::TsxNode>>().iter(app.world()).map(|e| e.to_bits()).collect();
    let held: std::collections::BTreeSet<u64> = client.values().copied().collect();
    assert_eq!(held, made);
    for name in ["EnemyA", "EnemyB"] {
        let (e, _) = named(&mut app, name)[0];
        assert!(held.contains(&e.to_bits()), "{name} is in the tree with its new entity");
    }
    assert!(!held.contains(&old_a.to_bits()));
}

/// After a Stop the engine's own entities (the viewport's window, bevy_picking's pointers) are not
/// listed, so the studio does not show them as spawned at runtime.
#[test]
fn engine_entities_stay_out_of_the_entity_list_after_stop() {
    let (_dir, mut app) = played("play-stop-engine");
    mode(&mut app, "edit");
    updates(&mut app, 5);
    let world = app.world_mut();
    let engine: std::collections::BTreeSet<u64> = world
        .query_filtered::<Entity, Or<(With<Window>, With<bevy::picking::pointer::PointerId>)>>()
        .iter(world)
        .map(|e| e.to_bits())
        .collect();
    assert!(engine.len() >= 2, "a window and a pointer: {engine:?}");
    let listed: std::collections::BTreeSet<u64> =
        call(&mut app, "bevy-ts/entities", Value::Null)["entities"].as_array().unwrap().iter().map(|e| e["entity"].as_u64().unwrap()).collect();
    assert!(!listed.is_empty());
    assert!(listed.is_disjoint(&engine), "listed: {:?}", listed.intersection(&engine).collect::<Vec<_>>());
    let id = match app.world().resource::<RemoteMethods>().get("bevy-ts/entities+watch") {
        Some(RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("bevy-ts/entities+watch not served"),
    };
    let opening = app.world_mut().run_system_with(id, None).unwrap().unwrap().unwrap();
    let watched: std::collections::BTreeSet<u64> = opening["snapshot"].as_array().unwrap().iter().map(|e| e["entity"].as_u64().unwrap()).collect();
    assert_eq!(watched, listed, "the stream opens with the same list");
    // A pointer the engine spawns while the stream is open is not sent; a scene entity is.
    let pointer = app.world_mut().spawn(bevy::picking::pointer::PointerId::Touch(99)).id();
    let prop = app.world_mut().spawn(Name::new("Prop")).id();
    let mut spawned = std::collections::BTreeSet::new();
    for _ in 0..3 {
        updates(&mut app, 1);
        let m = app.world_mut().run_system_with(id, None).unwrap().unwrap();
        spawned.extend(m.iter().flat_map(|m| m["spawned"].as_array().cloned().unwrap_or_default()).map(|e| e["entity"].as_u64().unwrap()));
    }
    assert!(spawned.contains(&prop.to_bits()), "the stream sends spawns: {spawned:?}");
    assert!(!spawned.contains(&pointer.to_bits()), "the pointer is the engine's");
}

#[test]
fn a_level_closed_during_play_opens_again_on_stop() {
    let (_dir, mut app) = played("play-stop-closed");
    let r = call(&mut app, "bevy-ts/level_close", json!({ "file": "levels/arena.bsn" }));
    assert_eq!(r["closed"], true, "{r}");
    assert!(named(&mut app, "EnemyA").is_empty());
    mode(&mut app, "edit");
    let levels = app.world().resource::<Levels>();
    assert_eq!(levels.roots.get("levels/arena.bsn").map(|r| r.opener), Some(Opener::Ts));
    assert!(levels.closed.is_empty());
    assert_eq!(named(&mut app, "EnemyA").len(), 1);
}

/// Task 5.8: `examples/rapier`, a project engine with the Rapier port, under `--viewport --edit` in
/// a process of its own. Play, Stop, play: the bodies move the same at each fixed step. Ignored:
/// the first run builds the project engine with Rapier.
mod rapier {
    use bevy_ts_viewport::mach::{Incoming, Viewer, release_port};
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::Path;
    use crate::common::{BOUND, Engine};
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

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
            assert!(t0.elapsed() < BOUND, "no frames: {seen} of {want}");
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

    /// Fixed time elapsed, in nanoseconds.
    fn fixed_ns(port: u16) -> u128 {
        let r = post(port, "world.get_resources", json!({ "resource": "bevy_time::time::Time<bevy_time::fixed::Fixed>" })).unwrap();
        let e = &r["value"]["elapsed"];
        let (s, n) = (e["secs"].as_u64(), e["nanos"].as_u64());
        match (s, n) {
            (Some(s), Some(n)) => s as u128 * 1_000_000_000 + n as u128,
            _ => panic!("Time<Fixed> reads {r}"),
        }
    }

    /// Every dynamic body's translation, sorted.
    fn bodies(port: u16) -> Vec<[f64; 3]> {
        let rb = "bevy_rapier3d::dynamics::rigid_body::RigidBody";
        let t = "bevy_transform::components::transform::Transform";
        let r = post(port, "world.query", json!({ "data": { "components": [rb, t] } })).unwrap();
        let mut out: Vec<[f64; 3]> = r
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["components"][rb] == "Dynamic")
            .map(|e| {
                let p = &e["components"][t]["translation"];
                [p[0].as_f64().unwrap(), p[1].as_f64().unwrap(), p[2].as_f64().unwrap()]
            })
            .collect();
        out.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out
    }

    /// Plays, pauses, then steps one frame at a time: the bodies at each fixed time reached.
    fn run(port: u16, viewer: &Viewer, seen: &mut usize, steps: usize) -> BTreeMap<u128, Vec<[f64; 3]>> {
        let r = post(port, "bevy-ts/viewport_mode", json!({ "mode": "play" })).unwrap();
        assert_eq!(r["started"], true);
        post(port, "bevy-ts/pause", json!({ "paused": true })).unwrap();
        frames(viewer, seen, 2);
        let mut out = BTreeMap::new();
        for _ in 0..steps {
            post(port, "bevy-ts/step", json!({ "frames": 1 })).unwrap();
            frames(viewer, seen, 2);
            out.insert(fixed_ns(port), bodies(port));
        }
        out
    }

    #[test]
    #[ignore = "builds the rapier project engine"]
    fn play_stop_play_runs_the_same() {
        let game = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rapier/src/game.tsx");
        let log = Path::new(env!("CARGO_TARGET_TMPDIR")).join("play-stop-rapier.log");
        let port = crate::common::free_port();
        let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let _engine = Engine::spawn(
            Command::new(env!("CARGO_BIN_EXE_bevy-ts"))
                .args(["dev", game.to_str().unwrap(), "--viewport", "--edit", "--remote-port", &port.to_string()])
                .env("CARGO_TARGET_DIR", target)
                .stdout(Stdio::null())
                .stderr(std::fs::File::create(&log).unwrap()),
        );
        let t0 = Instant::now();
        while post(port, "bevy-ts/hello", Value::Null).is_err() {
            assert!(t0.elapsed() < Duration::from_secs(1200), "the engine never answered");
            std::thread::sleep(Duration::from_millis(200));
        }
        let a = post(port, "bevy-ts/viewport_attach", json!({ "w": 320, "h": 240, "scale": 1, "hz": 120 })).unwrap();
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
        frames(&viewer, &mut seen, 20);
        assert!(bodies(port).is_empty(), "no Startup in edit");

        let first = run(port, &viewer, &mut seen, 60);
        let r = post(port, "bevy-ts/viewport_mode", json!({ "mode": "edit" })).unwrap();
        assert_eq!(r["started"], false);
        assert!(bodies(port).is_empty(), "the bodies went with their entities");
        assert_eq!(fixed_ns(port), 0, "fixed time starts again");
        let second = run(port, &viewer, &mut seen, 60);

        let both: Vec<&u128> = first.keys().filter(|t| second.contains_key(t)).collect();
        assert!(both.len() >= 10, "few fixed times seen in both runs: {} of {}", both.len(), first.len());
        for t in both {
            assert_eq!(first[t], second[t], "the bodies at fixed time {t} ns");
        }
        let moved = first.values().last().unwrap();
        assert!(moved.iter().any(|p| p[1] > 2.5), "the rocket climbed: {moved:?}");
        ticking.store(false, Ordering::Relaxed);
        ticker.join().unwrap();
        let log = std::fs::read_to_string(&log).unwrap();
        assert!(!log.contains("panicked"), "the engine panicked:\n{log}");
    }
}

/// `viewport_mode` as a client sees a refusal: the message and `data`.
fn refused(app: &mut App, mode: &str) -> Result<Value, (String, Option<Value>)> {
    let id = match app.world().resource::<RemoteMethods>().get("bevy-ts/viewport_mode") {
        Some(RemoteMethodSystemId::Instant(id)) => *id,
        _ => panic!("no viewport_mode"),
    };
    app.world_mut().run_system_with(id, Some(json!({ "mode": mode }))).unwrap().map_err(|e| (e.message, e.data))
}

#[test]
fn a_compile_that_failed_during_play_still_refuses_play_after_stop() {
    let (dir, mut app) = played("play-stop-typo");
    let watched = app.world().resource::<bevy_ts::generation::ReloadLog>().next_seq();
    write(&dir, "src/game.tsx", &format!("{GAME}\nexport const broken = ;\n"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Failed);
    assert_eq!(mode(&mut app, "edit")["started"], false);
    let (message, data) = refused(&mut app, "play").unwrap_err();
    assert_eq!(data, Some(json!({ "reason": "compile-errors" })));
    assert!(message.contains("game.tsx:"), "{message}");
    let reports: Vec<ReloadStatus> = app.world().resource::<bevy_ts::generation::ReloadLog>().since(watched).map(|r| r.status).collect();
    assert_eq!(reports, vec![ReloadStatus::Failed], "the Stop's rebuild is not a generation on reload+watch");
    // Fixed: the save applies and the game plays.
    write(&dir, "src/game.tsx", GAME);
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    assert_eq!(refused(&mut app, "play").unwrap()["mode"], "play");
}

#[test]
fn the_first_stop_of_a_game_booted_in_play_keeps_its_levels() {
    let dir = arena("play-stop-booted");
    let mut app = stop_app_booted(&dir, false);
    app.update();
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    until(&mut app, "the level never loaded", |app| named(app, "EnemyB").len() == 1);
    assert_eq!(call(&mut app, "bevy-ts/level_open", json!({ "file": "levels/room.bsn" }))["status"], "opened");
    updates(&mut app, 5);
    assert_eq!(mode(&mut app, "edit")["started"], false);
    let open: Vec<String> = app.world().resource::<Levels>().roots.keys().cloned().collect();
    assert_eq!(open, vec!["levels/arena.bsn".to_string(), "levels/room.bsn".to_string()]);
    assert_eq!((named(&mut app, "EnemyA").len(), named(&mut app, "Guard").len()), (1, 1), "each reconciled");
}

/// The arena (its HUD is game UI) in a fresh copy, booted as `bevy-ts dev --edit` boots.
fn arena_example(name: &str) -> App {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena");
    let dir = std::env::temp_dir().join("bevy-ts-cli-tests").join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    assert!(std::process::Command::new("cp").args(["-R", src.to_str().unwrap(), dir.to_str().unwrap()]).status().unwrap().success());
    let mut app = stop_app_entry(&dir, "src/game.tsx", true);
    app.update();
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    updates(&mut app, 30);
    app
}

/// Whether any game UI root targets a camera that renders.
fn ui_shows(app: &mut App) -> bool {
    let world = app.world_mut();
    let roots: Vec<Option<Entity>> = world.query_filtered::<&bevy::ui::ComputedUiTargetCamera, Without<ChildOf>>().iter(world).map(|t| t.get()).collect();
    assert!(!roots.is_empty(), "the HUD is mounted");
    roots.into_iter().flatten().any(|c| world.get::<Camera>(c).is_some_and(|c| c.is_active))
}

#[test]
fn game_ui_stays_hidden_in_edit_with_a_selection_and_after_stop() {
    let mut app = arena_example("play-stop-hud");
    assert!(!ui_shows(&mut app));
    let (b, _) = named(&mut app, "EnemyB")[0];
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [b.to_bits()] }));
    updates(&mut app, 5);
    assert!(!ui_shows(&mut app), "not on the gizmo's overlay camera either");
    mode(&mut app, "play");
    updates(&mut app, 5);
    assert!(ui_shows(&mut app), "the HUD draws in play");
    mode(&mut app, "edit");
    updates(&mut app, 5);
    assert!(!ui_shows(&mut app), "and not after Stop");
    call(&mut app, "bevy-ts/viewport_mode", json!({ "ui": true }));
    updates(&mut app, 5);
    assert!(ui_shows(&mut app), "ui: true shows it over the editor view");
}

// Change `game-camera-edit`: the game view on a real game.

/// One `viewport_mode+watch` subscriber's next message, if one is owed.
fn mode_watch(app: &mut App, key: &Value) -> Option<Value> {
    let id = match app.world().resource::<RemoteMethods>().get("bevy-ts/viewport_mode+watch") {
        Some(RemoteMethodSystemId::Watching(id)) => *id,
        _ => panic!("no mode watch"),
    };
    app.world_mut().run_system_with(id, Some(key.clone())).unwrap().unwrap()
}

/// The arena's scene camera counts in edit before any Play, and the game view draws it. Its level
/// gains a second camera through `level_write`: the watch says `cameras`, the game view shows it, and
/// the HUD draws there as in Play.
#[test]
fn a_level_camera_reaches_the_watch_and_the_game_view_draws_the_hud() {
    let mut app = arena_example("game-camera-hud");
    let key = json!({ "n": 1 });
    let first = mode_watch(&mut app, &key).expect("a message on open");
    assert_eq!((first["gameCameras"].as_u64(), first["cause"].as_str()), (Some(1), Some("engine")), "the arena's camera is its scene's: {first}");
    let r = call(&mut app, "bevy-ts/viewport_mode", json!({ "camera": "game" }));
    assert_eq!((r["camera"].as_str(), r["started"].as_bool(), r["gameCameras"].as_u64()), (Some("game"), Some(false), Some(1)), "{r}");
    updates(&mut app, 5);
    mode_watch(&mut app, &key);
    let world = app.world_mut();
    let own = world.query::<(Entity, &Name)>().iter(world).find(|(_, n)| n.as_str() == "camera").map(|(e, _)| e).expect("the scene's camera");
    assert!(world.get::<Camera>(own).unwrap().is_active, "the game view shows the scene's camera in edit");
    assert!(ui_shows(&mut app), "and the HUD over it");

    let base = call(&mut app, "bevy-ts/level_list", Value::Null)["levels"][0]["hash"].clone();
    let w = call(&mut app, "bevy-ts/level_write", json!({
        "file": "levels/arena.bsn", "base": base,
        "ops": [{ "op": "add_entity", "parent": [], "name": "Shot", "patches": [
            { "component": "Camera3d" },
            { "component": "Transform", "fields": { "translation": { "type": "Vec3", "lanes": { "x": -6.0, "y": 5.0, "z": 10.0 } } } },
        ] }],
    }));
    assert_eq!(w["status"], "written", "{w:#}");
    let mut seen = vec![];
    until(&mut app, "the written camera was never counted", |app| {
        seen.extend(mode_watch(app, &key));
        seen.iter().any(|m| m["gameCameras"] == 2)
    });
    let m = seen.iter().find(|m| m["gameCameras"] == 2).unwrap();
    assert_eq!((m["cause"].as_str(), m["camera"].as_str()), (Some("cameras"), Some("game")), "{m}");
    updates(&mut app, 5);
    let world = app.world_mut();
    let shot = world.query::<(Entity, &Name)>().iter(world).find(|(_, n)| n.as_str() == "Shot").map(|(e, _)| e).unwrap();
    assert!(world.get::<Camera>(shot).unwrap().is_active, "the game view shows it");
    assert!(ui_shows(&mut app), "the HUD draws in the game view, as in play");
    call(&mut app, "bevy-ts/viewport_mode", json!({ "camera": "editor" }));
    updates(&mut app, 5);
    assert!(!ui_shows(&mut app), "and not in the editor view");
}

// Change stop-incremental: a Stop restores the levels the play changed, keeping the rest.

const ARENA3: &str = "#EnemyA
@game::Enemy
Transform { translation: Vec3 { x: -3.0, z: -4.0 } }
--
#EnemyB
@game::Enemy
Transform { translation: Vec3 { x: 0.0, z: -4.0 } }
--
#EnemyC
@game::Enemy
Transform { translation: Vec3 { x: 3.0, z: -4.0 } }
--
#EnemyD
@game::Enemy
Transform { translation: Vec3 { x: 6.0, z: -4.0 } }
";

#[derive(Component)]
struct Stunned;

/// `Added<LevelInstance>` seen by a native system, frame by frame.
#[derive(Resource, Default)]
struct AddedSeen(usize);

fn count_added(q: Query<(), Added<bevy_ts_levels::engine::LevelInstance>>, mut seen: ResMut<AddedSeen>) {
    seen.0 += q.iter().count();
}

/// The three-enemy arena loaded in edit, `game` its source, `extra` added to the app.
fn arena3(name: &str, game: &str, extra: impl FnOnce(&mut App)) -> (std::path::PathBuf, App) {
    let dir = project(name, &[("src/game.tsx", game), ("assets/levels/arena.bsn", ARENA3)]);
    let mut app = stop_app(&dir);
    extra(&mut app);
    app.update();
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    for _ in 0..200 {
        if named(&mut app, "EnemyC").len() == 1 {
            break;
        }
        updates(&mut app, 1);
    }
    assert_eq!(named(&mut app, "EnemyC").len(), 1, "the level never loaded");
    updates(&mut app, 3);
    (dir, app)
}

fn enemy(app: &mut App, name: &str) -> Entity {
    named(app, name)[0].0
}

fn enemies(app: &mut App) -> Vec<Entity> {
    ["EnemyA", "EnemyB", "EnemyC"].iter().map(|n| enemy(app, n)).collect()
}

/// The level root's rows added or removed in `tsx_tree+watch` messages after `seq`.
fn level_rows_changed(app: &App, seq: u64) -> usize {
    let tree = app.world().resource::<bevy_ts_tsx::TsxTree>();
    tree.journal.messages.iter().filter(|(s, _)| *s > seq).map(|(_, c)| c.added.len() + c.removed.len()).sum()
}

#[test]
fn a_stop_after_a_play_that_changed_nothing_keeps_every_enemy() {
    let (_dir, mut app) = arena3("stop-kept-nothing", GAME, |app| {
        app.init_resource::<AddedSeen>().add_systems(Update, count_added);
    });
    let before = enemies(&mut app);
    mode(&mut app, "play");
    updates(&mut app, 10);
    let seq = app.world().resource::<bevy_ts_tsx::TsxTree>().journal.seq;
    let added = app.world().resource::<AddedSeen>().0;
    mode(&mut app, "edit");
    updates(&mut app, 3);
    assert_eq!(enemies(&mut app), before, "every enemy keeps its entity");
    assert_eq!(level_rows_changed(&app, seq), 0, "tsx_tree+watch reports nothing spawned or despawned");
    assert_eq!(bullets(&mut app), 0, "what TS spawned is still gone");
    mode(&mut app, "play");
    updates(&mut app, 3);
    assert_eq!(app.world().resource::<AddedSeen>().0, added, "a native system sees no new Added");
}

#[test]
fn a_stop_respawns_the_enemies_the_play_changed() {
    let (_dir, mut app) = arena3("stop-kept-some", GAME, |_| {});
    let [a, b, c] = enemies(&mut app)[..] else { unreachable!() };
    let d = enemy(&mut app, "EnemyD");
    mode(&mut app, "play");
    updates(&mut app, 3);
    app.world_mut().get_mut::<Transform>(b).unwrap().translation.y = 5.0;
    app.world_mut().entity_mut(c).insert(Stunned);
    app.world_mut().entity_mut(a).despawn();
    updates(&mut app, 3);
    mode(&mut app, "edit");
    let [a2, b2, c2] = enemies(&mut app)[..] else { unreachable!() };
    assert!(a2 != a && b2 != b && c2 != c, "each changed enemy is respawned");
    assert_eq!(enemy(&mut app, "EnemyD"), d, "the one the play did not change keeps its entity");
    assert_eq!(named(&mut app, "EnemyB")[0].1, Vec3::new(0.0, 0.0, -4.0), "at its level transform");
    assert!(app.world().get::<Stunned>(c2).is_none(), "without what the play added");
    assert!(app.world().resource::<bevy_ts_tsx::TsxTree>().tombstones.is_empty(), "no tombstone");
    assert!(app.world().get::<bevy_ts_levels::engine::LevelInstance>(a2).is_some(), "the respawned enemy is stamped");
}

#[test]
fn a_stop_keeps_the_selected_enemy_the_play_did_not_change() {
    let (_dir, mut app) = arena3("stop-kept-selection", GAME, |_| {});
    let c = enemy(&mut app, "EnemyC");
    call(&mut app, "bevy-ts/viewport_select", json!({ "entities": [c.to_bits()] }));
    updates(&mut app, 2);
    mode(&mut app, "play");
    updates(&mut app, 5);
    mode(&mut app, "edit");
    assert_eq!(enemy(&mut app, "EnemyC"), c);
    let sel = app.world().resource::<bevy_ts_viewport::edit::Selection>();
    assert_eq!(sel.items.iter().map(|(e, _)| *e).collect::<Vec<_>>(), vec![c], "selected with the same entity");
}

#[test]
fn a_step_before_play_counts_as_the_play() {
    let (_dir, mut app) = arena3("stop-kept-step", GAME, |_| {});
    let a = enemy(&mut app, "EnemyA");
    call(&mut app, "bevy-ts/step", json!({ "frames": 1 }));
    // What the step's systems did.
    let b = enemy(&mut app, "EnemyB");
    app.world_mut().get_mut::<Transform>(b).unwrap().translation.y = 2.0;
    updates(&mut app, 2);
    mode(&mut app, "play");
    updates(&mut app, 3);
    mode(&mut app, "edit");
    assert_eq!(named(&mut app, "EnemyB")[0].1, Vec3::new(0.0, 0.0, -4.0), "b is back at its level transform");
    assert_eq!(enemy(&mut app, "EnemyA"), a, "a is kept");
}

#[test]
fn a_level_saved_during_play_remounts_on_stop() {
    let (dir, mut app) = arena3("stop-saved", GAME, |_| {});
    let a = enemy(&mut app, "EnemyA");
    mode(&mut app, "play");
    updates(&mut app, 3);
    write(&dir, "assets/levels/arena.bsn", &format!("{ARENA3}--\n#EnemyE\n@game::Enemy\nTransform {{ translation: Vec3 {{ x: 9.0 }} }}\n"));
    for _ in 0..400 {
        if named(&mut app, "EnemyE").len() == 1 {
            break;
        }
        updates(&mut app, 1);
    }
    assert_eq!(named(&mut app, "EnemyE").len(), 1, "the save reloaded");
    mode(&mut app, "edit");
    assert_eq!(named(&mut app, "EnemyE").len(), 1, "e exists");
    assert_ne!(enemy(&mut app, "EnemyA"), a, "the arena remounted as if first");
}

#[test]
fn a_level_whose_prefab_has_a_handler_remounts_on_stop() {
    let game = GAME.replace(r#"<entity name="enemy">"#, r#"<entity name="enemy" onPointerClick={() => {}}>"#);
    let (_dir, mut app) = arena3("stop-handler", &game, |_| {});
    let a = enemy(&mut app, "EnemyA");
    mode(&mut app, "play");
    updates(&mut app, 3);
    mode(&mut app, "edit");
    assert_ne!(enemy(&mut app, "EnemyA"), a, "the arena remounted as if first");
}

/// Task 6.3: a level with its own camera and light, under rendering and Play's camera swap. What
/// Bevy derives every frame does not count; `Camera`, which Bevy writes every frame too, does.
#[test]
fn a_level_camera_respawns_and_its_enemies_stay() {
    let level = format!("{ARENA3}--\n#Cam\nCamera3d\nTransform {{ translation: Vec3 {{ y: 4.0, z: 6.0 }} }}\n--\n#Lamp\nPointLight\nTransform {{ translation: Vec3 {{ y: 3.0 }} }}\n");
    let dir = project("stop-level-camera", &[("src/game.tsx", GAME), ("assets/levels/arena.bsn", &level)]);
    let mut app = stop_app(&dir);
    app.update();
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    for _ in 0..200 {
        if named(&mut app, "Lamp").len() == 1 {
            break;
        }
        updates(&mut app, 1);
    }
    updates(&mut app, 5);
    let (before, lamp, cam) = (enemies(&mut app), enemy(&mut app, "Lamp"), enemy(&mut app, "Cam"));
    mode(&mut app, "play");
    updates(&mut app, 10);
    mode(&mut app, "edit");
    assert_eq!(enemies(&mut app), before, "the enemies are kept");
    assert_eq!(enemy(&mut app, "Lamp"), lamp, "and the light");
    assert_ne!(enemy(&mut app, "Cam"), cam, "the camera respawns");
}

/// Moves one named entity, as a native crate's `OnEnter` or `OnExit(Play)` reset might.
fn lift(name: &'static str) -> impl FnMut(Query<(&Name, &mut Transform)>) {
    move |mut q| {
        for (n, mut t) in &mut q {
            if n.as_str() == name {
                t.translation.y = 7.0;
            }
        }
    }
}

#[test]
fn what_on_exit_does_to_a_level_entity_is_undone() {
    let (_dir, mut app) = arena3("stop-on-exit", GAME, |app| {
        app.add_systems(OnExit(bevy_ts::EngineMode::Play), lift("EnemyD"));
    });
    let a = enemy(&mut app, "EnemyA");
    mode(&mut app, "play");
    updates(&mut app, 3);
    mode(&mut app, "edit");
    assert_eq!(named(&mut app, "EnemyD")[0].1, Vec3::new(6.0, 0.0, -4.0), "D is back at its level transform");
    assert_eq!(enemy(&mut app, "EnemyA"), a, "A is kept");
}

#[test]
fn what_on_enter_does_on_the_first_play_is_undone() {
    let (_dir, mut app) = arena3("stop-on-enter", GAME, |app| {
        app.add_systems(OnEnter(bevy_ts::EngineMode::Play), lift("EnemyC"));
    });
    let a = enemy(&mut app, "EnemyA");
    mode(&mut app, "play");
    updates(&mut app, 3);
    assert_eq!(named(&mut app, "EnemyC")[0].1.y, 7.0, "OnEnter moved it");
    mode(&mut app, "edit");
    assert_eq!(named(&mut app, "EnemyC")[0].1, Vec3::new(3.0, 0.0, -4.0), "C is back at its level transform");
    assert_eq!(enemy(&mut app, "EnemyA"), a, "A is kept");
}
