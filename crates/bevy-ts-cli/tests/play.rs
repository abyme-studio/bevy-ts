//! Change `build-command`, shipped-game: an engine plays a game folder made by hand (the cube,
//! compiled for release), with no compiler, watcher or protocol, and refuses another version's.

mod common;

use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::play::GameFile;
use common::{BIN, listening, play, wait_for};
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
    let cube = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/cube/src/game.ts")).unwrap();
    let project = common::project(name, &[("tsconfig.json", "{}"), ("src/game.ts", &format!("{cube}{CUBE_EXTRA}")), ("src/enemy.ts", ENEMY)]);
    let compiler = Compiler::for_release(project.join("src/game.ts")).unwrap();
    let set = compiler.compile_graph().unwrap();
    let game = project.join("dist/game");
    for m in &set.modules {
        common::write(&game.join("scripts"), &m.path.strip_prefix(&compiler.root).unwrap().to_string_lossy(), &m.code);
    }
    std::fs::create_dir_all(game.join("assets")).unwrap();
    // Finder litters folders; a shipped game ignores what is not a script.
    std::fs::write(game.join("scripts/src/.DS_Store"), [0xff, 0xfe, 0x00, 0x01]).unwrap();
    let file = GameFile { name: "cube".into(), entry: "src/game.ts".into(), bevy_ts: version.into(), engine: "prebuilt".into() };
    std::fs::write(game.join("game.json"), serde_json::to_string(&file).unwrap()).unwrap();
    game
}

#[test]
fn a_hand_made_cube_plays_without_dev_tooling() {
    let game = cube_game("play-cube", env!("CARGO_PKG_VERSION"));
    let (mut child, rx) = play(&game);
    let mut lines = wait_for(&rx, &["generation 1 applied", "cube system ran", "enemy exploded"], Duration::from_secs(60));
    std::thread::sleep(Duration::from_millis(300));
    lines.extend(rx.try_iter());
    let ports = listening(child.id());
    // A script edit does not reload: nothing watches the scripts.
    common::write(&game.join("scripts"), "src/enemy.ts", "export function report() {}\n");
    std::thread::sleep(Duration::from_millis(500));
    let _ = child.kill();
    let _ = child.wait();
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
