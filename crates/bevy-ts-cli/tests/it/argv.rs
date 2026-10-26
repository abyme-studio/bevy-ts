//! Change script-networking-2: `process.argv` in Node's shape under `bevy-ts dev`, `bevy-ts play`
//! and a shipped game. The parsing is unit-tested in `cli.rs`; this is what a script sees.

use crate::common;

use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::play::GameFile;
use common::{BIN, Engine, LAUNCH, wait_for};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;

const GAME: &str = "console.log(\"argv \" + JSON.stringify(process.argv));\nexport {};\n";

/// Starts `cmd` and returns the `process.argv` its game logged.
fn argv_of(cmd: &mut Command) -> Vec<String> {
    let mut engine = Engine::spawn(cmd.env("NO_COLOR", "1").stdout(Stdio::null()).stderr(Stdio::piped()));
    let (tx, rx) = channel();
    let err = engine.child().stderr.take().unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(err).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    let seen = wait_for(&rx, &["argv ["], LAUNCH);
    let line = seen.iter().find(|l| l.contains("argv [")).unwrap_or_else(|| panic!("no argv line:\n{}", seen.join("\n")));
    // The log line goes on after the JSON (the call site): read to its last `]`.
    let json = &line[line.find("argv [").unwrap() + 5..=line.rfind(']').unwrap()];
    serde_json::from_str(json).unwrap_or_else(|e| panic!("{e}: {line}"))
}

/// A game folder made by hand from [`GAME`], as `bevy-ts build` makes one.
fn argv_game(name: &str) -> PathBuf {
    let project = common::project(name, &[("tsconfig.json", "{}"), ("src/game.ts", GAME)]);
    let compiler = Compiler::for_release(project.join("src/game.ts")).unwrap();
    let set = compiler.compile_graph().unwrap();
    let game = project.join("dist/game");
    for m in &set.modules {
        common::write(&game.join("scripts"), &m.path.strip_prefix(&compiler.root).unwrap().to_string_lossy(), &m.code);
    }
    std::fs::create_dir_all(game.join("assets")).unwrap();
    let file = GameFile {
        name: "arena".into(),
        entry: "src/game.ts".into(),
        bevy_ts: env!("CARGO_PKG_VERSION").into(),
        engine: "prebuilt".into(),
        switches: Default::default(),
        diagnostics: vec![],
        net: Default::default(),
        env: vec![],
    };
    std::fs::write(game.join("game.json"), serde_json::to_string(&file).unwrap()).unwrap();
    game
}

fn ends_with(path: &str, tail: &str) -> bool {
    Path::new(path).is_absolute() && path.replace('\\', "/").ends_with(tail)
}

#[test]
fn arguments_under_dev_and_none() {
    let dir = common::project("argv-dev", &[("tsconfig.json", "{}"), ("src/main.ts", GAME)]);
    let entry = dir.join("src/main.ts");
    let argv = argv_of(Command::new(BIN).args(["dev", entry.to_str().unwrap(), "--headless", "--", "--level", "arena"]));
    assert_eq!(argv[2..], ["--level", "arena"]);
    assert!(ends_with(&argv[1], "src/main.ts"), "{argv:?}");
    assert!(Path::new(&argv[0]).is_absolute(), "{argv:?}");
    let argv = argv_of(Command::new(BIN).args(["dev", entry.to_str().unwrap(), "--headless"]));
    assert_eq!(argv.len(), 2, "{argv:?}");
}

#[test]
fn playing_a_folder_with_arguments() {
    let game = argv_game("argv-play");
    let argv = argv_of(Command::new(BIN).args(["play", game.to_str().unwrap(), "--headless", "--", "--seed", "7"]));
    assert_eq!(argv[2..], ["--seed", "7"]);
    assert!(ends_with(&argv[1], "game/scripts/src/game.ts"), "{argv:?}");
}

/// A shipped game passes every argument to the game: its app, as `run_engine` builds it for the
/// `game/` beside the executable, holds them after the entry. Not started: a shipped game plays in
/// a window, and no test opens one; `bevy-ts play --headless` above shows what a script sees.
#[test]
fn a_built_game_with_arguments() {
    use bevy_ts_cli::cli::{Display, Prebuilt};
    let game = argv_game("argv-shipped");
    let engine = bevy_ts_cli::cli::Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let args: Vec<String> = ["--server", "10.0.0.5", "dev"].map(String::from).to_vec();
    let app = bevy_ts_cli::play::app(&engine, Prebuilt, &game, Display::Headless, &args).unwrap();
    let argv = &app.world().resource::<bevy_ts::backend::ScriptArgs>().0;
    assert_eq!(argv[2..], ["--server", "10.0.0.5", "dev"]);
    assert!(ends_with(&argv[1], "game/scripts/src/game.ts"), "{argv:?}");
}
