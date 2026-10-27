//! Change pixel-art-2d, stage 1: `bevy-ts.json`'s `assetSources`, folders outside the project
//! loaded as `name://path` in dev and in a built game, and checked by `levels check`.

use crate::common::{self, BIN, TestDir};
use bevy::asset::LoadState;
use bevy::prelude::*;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts::log::LogLevel;
use bevy_ts_cli::cli::{Display, Engine, Prebuilt, base_app_with_sources};
use bevy_ts_cli::compile::Compiler;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

const GAME: &str = "import { app } from \"bevy-ts\";\nimport { Sprite } from \"bevy-ts/bevy\";\n\napp.scene(\"s\", () => (\n  <entity name=\"hero\"><Sprite image=\"src://hero.png\" /></entity>\n));\n";

/// A 1 x 1 PNG.
fn png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, 1, 1);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&[255, 0, 0, 255]).unwrap();
    out
}

/// A folder outside any project holding `hero.png`.
fn source(name: &str) -> TestDir {
    let dir = TestDir::new(name);
    std::fs::write(dir.join("hero.png"), png()).unwrap();
    dir
}

/// A project whose `bevy-ts.json` names source `src` at `folder`, its entry `src/game.tsx`.
fn project(name: &str, folder: &str) -> TestDir {
    let config = format!("{{ \"name\": \"hero\", \"entry\": \"src/game.tsx\", \"assetSources\": {{ \"src\": {} }} }}", Value::from(folder));
    common::project(name, &[("bevy-ts.json", &config), ("package.json", "{}"), ("src/game.tsx", GAME)])
}

fn engine() -> Engine {
    Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") }
}

/// The project's game under `bevy-ts dev`'s `base_app`, headless, its first load applied.
fn dev(project: &Path) -> App {
    let found = bevy_ts_cli::project::find(project).unwrap().unwrap();
    let assets = bevy_ts_cli::headless::asset_root(project);
    let mut app = base_app_with_sources(&found.asset_sources(), &engine(), Prebuilt, Display::Headless, Default::default(), Some(&assets), true, Default::default(), &[], false);
    app.insert_resource(DevSource(Arc::new(Compiler::new(project.join("src/game.tsx"), true).unwrap())));
    app.finish();
    app.cleanup();
    let r = common::reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app
}

/// The hero sprite's image load state, once it settles.
fn hero_settles(app: &mut App) -> LoadState {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        app.update();
        let world = app.world_mut();
        let image = world.query::<(&Name, &Sprite)>().iter(world).find(|(n, _)| n.as_str() == "hero").map(|(_, s)| s.image.clone());
        if let Some(image) = image {
            let state = app.world().resource::<AssetServer>().load_state(&image);
            if matches!(state, LoadState::Loaded | LoadState::Failed(_)) {
                return state;
            }
        }
        assert!(Instant::now() < deadline, "the hero's image never settled");
    }
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() { files(&p, out) } else { out.push(p) }
    }
}

#[test]
fn a_sprite_loads_from_a_folder_outside_the_project() {
    let art = source("sources-art");
    let game = project("sources-dev", art.to_str().unwrap());
    let _one = common::renderer();
    let mut app = dev(&game);
    assert!(matches!(hero_settles(&mut app), LoadState::Loaded), "src://hero.png loads from {}", art.display());
    let mut all = Vec::new();
    files(&game, &mut all);
    assert!(!all.iter().any(|f| f.ends_with("hero.png")), "nothing is copied under the project: {all:?}");
}

#[test]
fn a_missing_source_warns_in_dev_and_its_load_fails() {
    let game = project("sources-missing", "no-such-art");
    let _one = common::renderer();
    let mut app = dev(&game);
    let warnings: Vec<String> = common::logs(&app).into_iter().filter(|e| e.level == LogLevel::Warn && e.message.contains("asset source")).map(|e| e.message).collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("\"src\"") && warnings[0].contains("no-such-art"), "{}", warnings[0]);
    assert!(matches!(hero_settles(&mut app), LoadState::Failed(_)));
}

#[test]
fn a_bad_source_name_stops_dev_naming_it() {
    let game = common::project("sources-bad-name", &[("bevy-ts.json", r#"{ "assetSources": { "Stardew": "art" } }"#), ("package.json", "{}"), ("game.tsx", "export const a = 1;\n")]);
    let out = Command::new(BIN).args(["dev", "--headless"]).arg(game.join("game.tsx")).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("assetSources") && err.contains("\"Stardew\""), "{err}");
}

/// `bevy-ts build` of `project` into `<project>/dist/<leaf>`, with `flags`: its JSON events.
fn build(project: &Path, leaf: &str, flags: &[&str]) -> (PathBuf, Vec<Value>) {
    let out = project.join("dist").join(leaf);
    let run = Command::new(BIN).arg("build").arg(project).args(["--json", "--out"]).arg(&out).args(flags).output().unwrap();
    let events: Vec<Value> = String::from_utf8_lossy(&run.stdout).lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    assert_eq!(run.status.code(), Some(0), "{events:#?}\n{}", String::from_utf8_lossy(&run.stderr));
    (out.join("game"), events)
}

fn played(game: &Path) -> Result<App, String> {
    bevy_ts_cli::play::app(&engine(), Prebuilt, game, Display::Headless, &[])
}

/// A played game's hero settles loaded.
fn plays_the_hero(game: &Path) {
    let mut app = played(game).unwrap();
    app.finish();
    app.cleanup();
    // play queues its one generation; the first frames apply it.
    assert!(matches!(hero_settles(&mut app), LoadState::Loaded));
}

#[test]
fn a_default_build_copies_no_source_and_plays_from_its_folder() {
    let scratch = TestDir::new("sources-build");
    let art = scratch.join("art");
    std::fs::create_dir_all(&art).unwrap();
    std::fs::write(art.join("hero.png"), png()).unwrap();
    let game_dir = scratch.join("game");
    for (f, src) in [("bevy-ts.json", r#"{ "name": "hero", "entry": "src/game.tsx", "assetSources": { "src": "../art" } }"#), ("package.json", "{}"), ("src/game.tsx", GAME)] {
        common::write(&game_dir, f, src);
    }
    let (game, _) = build(&game_dir, "plain", &[]);
    let mut all = Vec::new();
    files(game.parent().unwrap(), &mut all);
    assert!(!all.iter().any(|f| f.ends_with("hero.png")), "no source file in the build: {all:?}");
    assert!(!game.join("sources").exists());
    let file: Value = serde_json::from_str(&std::fs::read_to_string(game.join("game.json")).unwrap()).unwrap();
    let folder = file["assetSources"]["src"].as_str().unwrap_or_else(|| panic!("{file}"));
    assert_eq!(Path::new(folder), art.canonicalize().unwrap(), "a relative folder is made absolute");
    {
        let _one = common::renderer();
        plays_the_hero(&game);
    }

    std::fs::remove_dir_all(&art).unwrap();
    let Err(e) = played(&game) else { panic!("a game without its source does not start") };
    assert!(e.contains("\"src\"") && e.contains(folder) && e.contains("--copy-sources"), "{e}");
    let out = Command::new(BIN).arg("play").arg(&game).arg("--headless").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(err.lines().filter(|l| l.contains("asset source")).count(), 1, "{err}");
}

#[test]
fn a_copy_sources_build_holds_its_files_and_plays_without_the_folder() {
    let scratch = TestDir::new("sources-copy");
    let art = scratch.join("art");
    std::fs::create_dir_all(art.join("Maps")).unwrap();
    std::fs::write(art.join("hero.png"), png()).unwrap();
    std::fs::write(art.join("Maps/Town.png"), png()).unwrap();
    let game_dir = scratch.join("game");
    let config = format!("{{ \"name\": \"hero\", \"entry\": \"src/game.tsx\", \"assetSources\": {{ \"src\": {} }} }}", Value::from(art.to_str().unwrap()));
    for (f, src) in [("bevy-ts.json", config.as_str()), ("package.json", "{}"), ("src/game.tsx", GAME)] {
        common::write(&game_dir, f, src);
    }
    let (game, events) = build(&game_dir, "copied", &["--copy-sources"]);
    assert!(game.join("sources/src/hero.png").is_file() && game.join("sources/src/Maps/Town.png").is_file());
    let file: Value = serde_json::from_str(&std::fs::read_to_string(game.join("game.json")).unwrap()).unwrap();
    assert_eq!(file["assetSources"]["src"], "sources/src", "{file}");
    let logs: Vec<&str> = events.iter().filter(|e| e["event"] == "log").filter_map(|e| e["line"].as_str()).collect();
    assert!(logs.iter().any(|l| l.contains("asset source src: 2 files")), "{logs:?}");
    std::fs::remove_dir_all(&art).unwrap();
    let _one = common::renderer();
    plays_the_hero(&game);
}

#[test]
fn levels_check_reads_a_source_from_bevy_ts_json() {
    let art = source("sources-levels-art");
    let game = project("sources-levels", art.to_str().unwrap());
    common::write(&game, "assets/levels/a.bsn", "#Hero Sprite { image: \"src://hero.png\" }\n#Gone Sprite { image: \"src://gone.png\" }\n");
    let check = |file: &str| Command::new(BIN).current_dir(&*game).args(["levels", "check", file]).output().unwrap();
    let out = check("assets/levels/a.bsn");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert_eq!(stdout.lines().collect::<Vec<_>>(), ["assets/levels/a.bsn:2:23: warning: missing asset src://gone.png"]);

    common::write(&game, "assets/levels/b.bsn", "#Hero Sprite { image: \"nope://hero.png\" }\n");
    let out = check("assets/levels/b.bsn");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("\"nope\"") && stdout.contains("nope://hero.png"), "{stdout}");
}
