//! Change `build-command`, game-build: `bevy-ts build` writes a whole game folder or none, reports
//! its four steps as JSON lines, and lists its targets.

use crate::common;

use common::{BIN, LAUNCH, listening, play, wait_for};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn build(args: &[&str]) -> (Option<i32>, Vec<Value>) {
    let out = Command::new(BIN).arg("build").args(args).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let events = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l:?}\nstderr: {}", String::from_utf8_lossy(&out.stderr))))
        .collect();
    (out.status.code(), events)
}

fn of<'a>(events: &'a [Value], kind: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["event"] == kind).collect()
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() { files(&p, out) } else { out.push(p) }
    }
}

fn tmp(name: &str) -> PathBuf {
    // Per process: another worktree's run of the same test must not share, or delete, its files.
    let d = std::env::temp_dir().join("bevy-ts-cli-tests").join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn the_arena_builds_and_plays() {
    let arena = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena");
    // Canonical, as the build reports it (macOS /var is /private/var).
    let out = tmp("build-arena").canonicalize().unwrap().join("arena");
    let (code, events) = build(&[arena.to_str().unwrap(), "--json", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{events:#?}");
    let steps: Vec<_> = of(&events, "step").iter().map(|s| (s["index"].as_u64().unwrap(), s["total"].as_u64().unwrap())).collect();
    assert_eq!(steps, vec![(1, 4), (2, 4), (3, 4), (4, 4)]);
    let last = events.last().unwrap();
    assert_eq!(last["event"], "done", "{last}");
    let run = PathBuf::from(last["run"].as_str().unwrap());
    assert_eq!(run, out.join("arena"));
    assert!(run.is_file());
    assert_eq!(PathBuf::from(last["out"].as_str().unwrap()), out);
    let game = out.join("game");
    let file: Value = serde_json::from_str(&std::fs::read_to_string(game.join("game.json")).unwrap()).unwrap();
    assert_eq!(file["name"], "arena");
    assert_eq!(file["entry"], "src/game.tsx");
    assert_eq!(file["bevyTs"], env!("CARGO_PKG_VERSION"));
    assert_eq!(file["engine"], "prebuilt");
    assert!(game.join("scripts/src/enemy.tsx").is_file());
    assert!(game.join("assets/levels/arena.bsn").is_file());
    // No file of the folder holds the TS source of the entry.
    let source = std::fs::read(arena.join("src/game.tsx")).unwrap();
    let mut all = Vec::new();
    files(&out, &mut all);
    for f in &all {
        let bytes = std::fs::read(f).unwrap();
        assert!(!bytes.windows(source.len()).any(|w| w == source), "{} holds src/game.tsx", f.display());
    }

    let (engine, rx) = play(&game);
    let lines = wait_for(&rx, &["generation 1 applied: 4 systems"], LAUNCH);
    std::thread::sleep(Duration::from_secs(2));
    let ports = listening(engine.id());
    drop(engine);
    let rest: Vec<String> = rx.try_iter().collect();
    assert!(ports.is_empty(), "the game listens on {ports:?}");
    let errors: Vec<_> = lines.iter().chain(&rest).filter(|l| l.contains("ERROR")).collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn a_failed_build_keeps_the_earlier_folder() {
    let project = common::project(
        "build-broken",
        &[
            ("tsconfig.json", "{}"),
            ("src/game.ts", "import { a } from \"./a.ts\";\nimport { b } from \"./b.ts\";\nexport const c = a + b;\n"),
            ("src/a.ts", "export const a = 1;\n"),
            ("src/b.ts", "export const b = 2;\n"),
        ],
    );
    let out = project.join("dist/here");
    let (code, _) = build(&[project.to_str().unwrap(), "--json", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(0));
    let before = std::fs::read(out.join("game/game.json")).unwrap();

    common::write(&project, "src/a.ts", "export const a = ;\n");
    common::write(&project, "src/b.ts", "export const b = 2;\n\nconst = 3;\n");
    let (code, events) = build(&[project.to_str().unwrap(), "--json", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(1), "{events:#?}");
    assert_eq!(events[0]["event"], "step");
    assert_eq!(events[0]["index"], 1);
    let problems = of(&events, "problem");
    assert_eq!(problems.len(), 2, "{problems:#?}");
    let b = problems.iter().find(|p| p["file"].as_str().unwrap().ends_with("src/b.ts")).expect("a problem in b.ts");
    assert!(Path::new(b["file"].as_str().unwrap()).is_absolute());
    assert_eq!((b["line"].as_u64(), b["column"].as_u64()), (Some(3), Some(7)), "{b}");
    assert_eq!(events.last().unwrap()["event"], "failed");
    assert!(of(&events, "step").len() == 1);

    assert_eq!(std::fs::read(out.join("game/game.json")).unwrap(), before);
    let left: Vec<_> = std::fs::read_dir(project.join("dist")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(left, vec!["here"], "a partial folder was left");
}

#[test]
fn targets_list_five_and_only_the_host_builds() {
    let out = Command::new(BIN).args(["build", "--targets", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.lines().count(), 1);
    let v: Value = serde_json::from_str(stdout.trim()).unwrap();
    let targets = v["targets"].as_array().unwrap();
    assert_eq!(targets.len(), 5);
    let ready: Vec<_> = targets.iter().filter(|t| t["ready"] == true).collect();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0]["host"], true);
    assert!(ready[0]["why"].is_null());
    let other = targets.iter().find(|t| t["ready"] == false).unwrap();
    assert_eq!(other["why"], "cross builds are not supported yet");

    let id = other["id"].as_str().unwrap();
    let (code, events) = build(&["--target", id, "--json"]);
    assert_eq!(code, Some(1));
    assert_eq!(events, vec![serde_json::json!({ "event": "failed", "message": format!("{id}: cross builds are not supported yet") })]);
}

#[test]
fn a_shader_ships_as_its_file_and_plays_from_there() {
    let project = common::project(
        "build-shader",
        &[
            ("tsconfig.json", "{}"),
            ("src/game.ts", "import glow from \"./glow.wgsl\";\nexport const g = glow;\n"),
            ("src/glow.wgsl", "fn glow() {}\n"),
        ],
    );
    let out = project.join("dist/here");
    let (code, events) = build(&[project.to_str().unwrap(), "--json", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{events:#?}");
    let shipped = out.join("game/scripts/src/glow.wgsl");
    assert_eq!(std::fs::read_to_string(&shipped).unwrap(), "fn glow() {}\n");
    let game = out.join("game");
    let file = bevy_ts_cli::play::read_game(&game).unwrap();
    let set = bevy_ts_cli::play::read_scripts(&game, &file).unwrap();
    let m = set.modules.iter().find(|m| m.path.ends_with("src/glow.wgsl")).expect("the shader's module");
    assert!(m.code.contains("\"shaderFile\"") && m.code.contains(&*m.path.to_string_lossy()), "{}", m.code);
    assert!(m.path.starts_with(game.canonicalize().unwrap()), "{}", m.path.display());
}


/// script-packages: a linked package that reaches outside itself ships under
/// `node_modules/<name>@<version>/`, and the folder plays with the project's `node_modules` gone.
#[cfg(unix)]
#[test]
fn a_linked_package_ships_and_plays_without_node_modules() {
    let base = tmp("build-packages").canonicalize().unwrap();
    let write = |f: &str, src: &str| common::write(&base, f, src);
    write("web/packages/client/package.json", r#"{"name": "@abyme/client", "version": "0.0.0", "type": "module", "exports": {"./connect": "./src/connect.ts"}}"#);
    write(
        "web/packages/client/src/connect.ts",
        "import { PROTOCOL } from \"../../kernel/src/protocol.ts\";\nexport function connect(): string { return `connected over ${PROTOCOL}`; }\n",
    );
    write("web/packages/kernel/package.json", r#"{"name": "@abyme/kernel", "version": "0.0.0", "type": "module"}"#);
    write("web/packages/kernel/src/protocol.ts", "export const PROTOCOL: string = \"v1\";\n");
    write("app/package.json", r#"{"name": "linked", "type": "module"}"#);
    write("app/src/game.ts", "import { connect } from \"@abyme/client/connect\";\nconsole.log(connect());\n");
    std::fs::create_dir_all(base.join("app/node_modules/@abyme")).unwrap();
    std::os::unix::fs::symlink(base.join("web/packages/client"), base.join("app/node_modules/@abyme/client")).unwrap();

    let out = base.join("dist");
    let (code, events) = build(&[base.join("app").to_str().unwrap(), "--json", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{events:#?}");
    std::fs::remove_dir_all(base.join("app/node_modules")).unwrap();
    let game = out.join("game");
    let scripts = game.join("scripts");
    for f in ["src/game.ts", "node_modules/@abyme/client@0.0.0/src/connect.ts", "node_modules/@abyme/kernel@0.0.0/src/protocol.ts"] {
        assert!(scripts.join(f).is_file(), "{f}");
    }
    let mut all = Vec::new();
    files(&scripts, &mut all);
    let machine = base.to_string_lossy().to_string();
    for f in &all {
        let text = std::fs::read_to_string(f).unwrap();
        assert!(!text.contains(&machine), "{} names the build machine's {machine}", f.display());
        let map = bevy_ts_cli::compile::inline_source_map(&text).unwrap();
        assert!(!map.contains(&machine), "{}'s source map names {machine}", f.display());
    }
    let file: bevy_ts_cli::play::GameFile = serde_json::from_str(&std::fs::read_to_string(game.join("game.json")).unwrap()).unwrap();
    let set = bevy_ts_cli::play::read_scripts(&game, &file).unwrap();
    assert_eq!(set.modules.len(), 3);

    let (_engine, rx) = play(&game);
    wait_for(&rx, &["connected over v1"], LAUNCH);
}

#[test]
fn mesh_picking_is_baked_into_game_json_and_played() {
    use bevy::picking::mesh_picking::MeshPickingSettings;
    use bevy_ts_cli::cli::{Display, Engine, Prebuilt};
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    for (name, config, on) in [("build-picking-on", r#"{ "entry": "src/game.ts", "meshPicking": true }"#, true), ("build-picking-off", r#"{ "entry": "src/game.ts" }"#, false)] {
        let project = common::project(name, &[("bevy-ts.json", config), ("tsconfig.json", "{}"), ("src/game.ts", "export const a = 1;\n")]);
        let out = project.join("dist/here");
        let (code, events) = build(&[project.to_str().unwrap(), "--json", "--out", out.to_str().unwrap()]);
        assert_eq!(code, Some(0), "{events:#?}");
        let game = out.join("game");
        let file: Value = serde_json::from_str(&std::fs::read_to_string(game.join("game.json")).unwrap()).unwrap();
        assert_eq!(file.get("meshPicking"), on.then_some(&Value::Bool(true)), "{file}");
        let app = bevy_ts_cli::play::app(&engine, Prebuilt, &game, Display::Headless).unwrap();
        assert_eq!(app.world().contains_resource::<MeshPickingSettings>(), on, "{name}");
    }
}
