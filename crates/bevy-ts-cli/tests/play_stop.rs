//! Change `play-stop`: Stop rebuilds the edit world from source. A game on V8 with TSX and a
//! level, in the app `bevy-ts dev --viewport` builds, played and stopped through the protocol.
#![cfg(target_os = "macos")]

mod common;

use bevy::prelude::*;
use bevy::remote::{RemoteMethodSystemId, RemoteMethods, RemotePlugin};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::commands::TsSpawned;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_levels::engine::{Levels, LevelsPlugin, Opener};
use bevy_ts_v8::V8Options;
use common::{project, read_res, reload, write};
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
    for _ in 0..200 {
        if named(&mut app, "EnemyA").len() == 1 && named(&mut app, "EnemyB").len() == 1 {
            break;
        }
        updates(&mut app, 1);
    }
    assert_eq!(named(&mut app, "EnemyB").len(), 1, "the level never loaded");
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
    use std::process::{Child, Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

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

    /// Waits for `n` more frames from the viewer.
    fn frames(viewer: &Viewer, seen: &mut usize, n: usize) {
        let want = *seen + n;
        let t0 = Instant::now();
        while *seen < want {
            assert!(t0.elapsed() < Duration::from_secs(60), "no frames: {seen} of {want}");
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
        let port = 16160 + (std::process::id() % 40) as u16;
        let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let child = Command::new(env!("CARGO_BIN_EXE_bevy-ts"))
            .args(["dev", game.to_str().unwrap(), "--viewport", "--edit", "--remote-port", &port.to_string()])
            .env("CARGO_TARGET_DIR", target)
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&log).unwrap())
            .spawn()
            .unwrap();
        let _engine = Engine(child);
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
    for _ in 0..200 {
        if named(&mut app, "EnemyB").len() == 1 {
            break;
        }
        updates(&mut app, 1);
    }
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
