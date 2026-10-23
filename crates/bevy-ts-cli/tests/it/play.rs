//! Change `build-command`, shipped-game: an engine plays a game folder made by hand (the cube,
//! compiled for release), with no compiler, watcher or protocol, and refuses another version's.

use crate::common;

use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::play::GameFile;
use common::{BIN, BOUND, Engine, LAUNCH, listening, play, wait_for};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// The cube, with a system that says it ran and one in another module that throws.
const CUBE_EXTRA: &str = r#"
import { report } from "./enemy.ts";
export function ran(time: Res<Time>) {
  if (time.elapsed_secs > 0.5) console.log("cube system ran");
}
app.addSystems(Update, ran, report);
"#;

const ENEMY: &str = r#"import type { Res } from "bevy-ts";
import { Time } from "bevy-ts";

export function report(time: Res<Time>) {
  if (time.elapsed_secs > 1) {
    throw new Error("enemy exploded");
  }
}
"#;

/// A game folder for the cube, written file by file as `bevy-ts build` would.
fn cube_game(name: &str, version: &str) -> PathBuf {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cube/src");
    let cube = std::fs::read_to_string(example.join("game.ts")).unwrap();
    let scene = std::fs::read_to_string(example.join("cube.tsx")).unwrap();
    let project = common::project(
        name,
        &[("tsconfig.json", "{}"), ("src/game.ts", &format!("{cube}{CUBE_EXTRA}")), ("src/cube.tsx", &scene), ("src/enemy.ts", ENEMY)],
    );
    let compiler = Compiler::for_release(project.join("src/game.ts")).unwrap();
    let set = compiler.compile_graph().unwrap();
    let game = project.join("dist/game");
    for m in &set.modules {
        common::write(&game.join("scripts"), &m.path.strip_prefix(&compiler.root).unwrap().to_string_lossy(), &m.code);
    }
    std::fs::create_dir_all(game.join("assets")).unwrap();
    // Finder litters folders; a shipped game ignores what is not a script.
    std::fs::write(game.join("scripts/src/.DS_Store"), [0xff, 0xfe, 0x00, 0x01]).unwrap();
    let file = GameFile { name: "cube".into(), entry: "src/game.ts".into(), bevy_ts: version.into(), engine: "prebuilt".into(), mesh_picking: false, net: Default::default(), env: vec![] };
    std::fs::write(game.join("game.json"), serde_json::to_string(&file).unwrap()).unwrap();
    game
}

#[test]
fn a_hand_made_cube_plays_without_dev_tooling() {
    let game = cube_game("play-cube", env!("CARGO_PKG_VERSION"));
    let (engine, rx) = play(&game);
    // Launch to ready can stall for minutes on a loaded Mac's first launch of the binary (see
    // `LAUNCH`); once the first generation applies, the game runs within seconds or it is broken.
    let mut lines = wait_for(&rx, &["generation 1 applied"], LAUNCH);
    common::wait_more(&rx, &mut lines, &["cube system ran", "enemy exploded", "src/enemy.ts:6:"], BOUND);
    let ports = listening(engine.id());
    // A script edit does not reload: nothing watches the scripts.
    common::write(&game.join("scripts"), "src/enemy.ts", "export function report() {}\n");
    std::thread::sleep(Duration::from_millis(500));
    drop(engine);
    let later: Vec<String> = rx.try_iter().collect();
    assert!(ports.is_empty(), "the game listens on {ports:?}");
    assert!(!later.iter().chain(&lines).any(|l| l.contains("generation 2")), "{later:?}");
    // The thrown error names the TS file and line through the shipped source map.
    let at = lines.iter().position(|l| l.contains("enemy exploded")).unwrap();
    let stack = lines[at..].iter().take(3).cloned().collect::<Vec<_>>().join("\n");
    assert!(stack.contains("src/enemy.ts:6:"), "{stack}");
}

#[test]
fn a_played_game_carries_no_studio_tools_or_protocol() {
    use bevy_ts_cli::cli::{Display, Engine, Prebuilt, studio_tools};
    assert!(!studio_tools(Display::Window, false));
    assert!(studio_tools(Display::Window, true));
    let game = cube_game("play-tools", env!("CARGO_PKG_VERSION"));
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let app = bevy_ts_cli::play::app(&engine, Prebuilt, &game, Display::Headless).unwrap();
    let world = app.world();
    assert!(!world.contains_resource::<bevy::remote::RemoteMethods>(), "the protocol is served");
    assert!(!world.resource::<bevy_ts::runtime::RuntimeInfo>().dev);
    #[cfg(target_os = "macos")]
    {
        assert!(!world.contains_resource::<bevy_ts_viewport::assets::AssetRoot>());
        assert!(!world.contains_resource::<bevy_ts_viewport::shared::SharedRes>());
    }
}

#[test]
fn a_mismatched_engine_refuses_naming_both_versions() {
    let game = cube_game("play-mismatch", "0.0.1");
    let out = Command::new(BIN).args(["play", game.to_str().unwrap(), "--headless"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("0.0.1") && err.contains(env!("CARGO_PKG_VERSION")), "{err}");
}

#[test]
fn a_bare_start_with_no_game_prints_the_usage() {
    let out = Command::new(BIN).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("bevy-ts play <game folder>"));
}

#[test]
fn game_json_switches_mesh_picking() {
    use bevy::picking::mesh_picking::MeshPickingSettings;
    use bevy_ts_cli::cli::{Display, Engine, Prebuilt};
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    // A game.json written before the switch has no `meshPicking`, as one built without it.
    let game = cube_game("play-picking", env!("CARGO_PKG_VERSION"));
    let text = std::fs::read_to_string(game.join("game.json")).unwrap();
    assert!(!text.contains("meshPicking"), "{text}");
    let app = bevy_ts_cli::play::app(&engine, Prebuilt, &game, Display::Headless).unwrap();
    assert!(!app.world().contains_resource::<MeshPickingSettings>());

    let mut file = bevy_ts_cli::play::read_game(&game).unwrap();
    file.mesh_picking = true;
    std::fs::write(game.join("game.json"), serde_json::to_string(&file).unwrap()).unwrap();
    assert!(std::fs::read_to_string(game.join("game.json")).unwrap().contains(r#""meshPicking":true"#));
    let app = bevy_ts_cli::play::app(&engine, Prebuilt, &game, Display::Headless).unwrap();
    assert!(app.world().contains_resource::<MeshPickingSettings>());
}

/// A test that panics with an engine up leaves none behind, nor anything the engine started; and
/// the engine shares the runner's process group, so a signal to `cargo test` reaches it too.
#[cfg(unix)]
#[test]
fn a_panicking_test_leaves_no_engine_behind() {
    let gone = |pid: u32| {
        // An orphan is a zombie until launchd reaps it.
        let t0 = std::time::Instant::now();
        while Engine::alive(pid) && t0.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        !Engine::alive(pid)
    };
    let game = cube_game("play-panics", env!("CARGO_PKG_VERSION"));
    let (tx, rx) = std::sync::mpsc::channel();
    let panicked = std::thread::spawn(move || {
        let (engine, lines) = play(&game);
        assert_eq!(Engine::group(engine.id()), Engine::group(std::process::id()), "the engine is in the runner's group");
        tx.send(engine.id()).unwrap();
        wait_for(&lines, &["generation 1 applied"], LAUNCH);
        panic!("the test fails with the engine up");
    })
    .join();
    assert!(panicked.is_err());
    let pid = rx.recv().unwrap();
    assert!(gone(pid), "engine {pid} outlived the panic");

    // A supervisor's child dies with it.
    let (tx, rx) = std::sync::mpsc::channel();
    let panicked = std::thread::spawn(move || {
        let engine = Engine::spawn(Command::new("sh").args(["-c", "sleep 600 & wait"]));
        let t0 = std::time::Instant::now();
        let under = loop {
            let under = Engine::descendants(engine.id());
            if !under.is_empty() || t0.elapsed() > BOUND {
                break under;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(!under.is_empty(), "sh never started its child");
        tx.send(under).unwrap();
        panic!("the test fails with a supervised engine up");
    })
    .join();
    assert!(panicked.is_err());
    for pid in rx.recv().unwrap() {
        assert!(gone(pid), "{pid}, under the engine, outlived the panic");
    }
}
