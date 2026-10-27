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

#[test]
fn the_arena_builds_and_plays() {
    let arena = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/arena");
    // Canonical, as the build reports it (macOS /var is /private/var).
    let _scratch = common::TestDir::new("build-arena");
    let out = _scratch.join("arena");
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
    let lines = wait_for(&rx, &["generation 1 applied: 3 systems"], LAUNCH);
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
    let base = common::TestDir::new("build-packages");
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
fn switches_are_baked_into_game_json_and_played() {
    use bevy::input_focus::directional_navigation::{DirectionalNavigationMap, DirectionalNavigationPlugin};
    use bevy::input_focus::tab_navigation::TabNavigationPlugin;
    use bevy::picking::mesh_picking::MeshPickingSettings;
    use bevy_ts_cli::cli::{Display, Engine, Prebuilt};
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let all = r#"{ "entry": "src/game.ts", "meshPicking": true, "tabNavigation": true, "directionalNavigation": true, "imageSampler": "nearest" }"#;
    for (name, config, on) in [("build-switches-on", all, true), ("build-switches-off", r#"{ "entry": "src/game.ts" }"#, false)] {
        let project = common::project(name, &[("bevy-ts.json", config), ("tsconfig.json", "{}"), ("src/game.ts", "export const a = 1;\n")]);
        let out = project.join("dist/here");
        let (code, events) = build(&[project.to_str().unwrap(), "--json", "--out", out.to_str().unwrap()]);
        assert_eq!(code, Some(0), "{events:#?}");
        let game = out.join("game");
        let file: Value = serde_json::from_str(&std::fs::read_to_string(game.join("game.json")).unwrap()).unwrap();
        for key in ["meshPicking", "tabNavigation", "directionalNavigation"] {
            assert_eq!(file.get(key), on.then_some(&Value::Bool(true)), "{key} in {file}");
        }
        assert_eq!(file.get("imageSampler").and_then(Value::as_str), on.then_some("nearest"), "{file}");
        let app = bevy_ts_cli::play::app(&engine, Prebuilt, &game, Display::Headless, &[]).unwrap();
        assert_eq!(app.world().contains_resource::<MeshPickingSettings>(), on, "{name}");
        assert_eq!(app.is_plugin_added::<TabNavigationPlugin>(), on, "{name}");
        assert_eq!(app.is_plugin_added::<DirectionalNavigationPlugin>(), on, "{name}");
        assert_eq!(app.world().contains_resource::<DirectionalNavigationMap>(), on, "{name}");
        let filter = app.get_added_plugins::<bevy::image::ImagePlugin>()[0].default_sampler.mag_filter;
        assert_eq!(filter, if on { bevy::image::ImageFilterMode::Nearest } else { bevy::image::ImageFilterMode::Linear }, "{name}");
    }
}

/// A project engine's game plays on its own executable: examples/thrusters built in release (its
/// native crate and Rapier), played headless by the binary beside its `game/`, applies generation
/// 1 and runs its Rust (`thrusters: rocket`). Slow (a release project-engine build):
/// `cargo test --release -p bevy-ts-cli --test it thrusters_plays -- --ignored`.
#[test]
#[ignore = "builds the thrusters project engine in release"]
fn thrusters_plays_on_its_own_engine() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/thrusters").canonicalize().unwrap();
    let scratch = common::TestDir::new("thrusters-dist");
    let out = scratch.join("dist");
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let built = Command::new(BIN).args(["build", example.to_str().unwrap(), "--out", out.to_str().unwrap()]).env("CARGO_TARGET_DIR", &target).output().unwrap();
    assert!(built.status.success(), "build failed:\n{}", String::from_utf8_lossy(&built.stderr));
    let exe = out.join(if cfg!(windows) { "thrusters.exe" } else { "thrusters" });
    let mut engine = common::Engine::spawn(
        Command::new(&exe)
            .args(["play", out.join("game").to_str().unwrap(), "--headless"])
            .env("NO_COLOR", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped()),
    );
    let (tx, rx) = std::sync::mpsc::channel();
    let err = engine.child().stderr.take().unwrap();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for l in std::io::BufReader::new(err).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    let seen = wait_for(&rx, &["generation 1 applied", "thrusters: rocket"], LAUNCH);
    assert!(!seen.iter().any(|l| l.contains("failed")), "a failed generation:\n{}", seen.join("\n"));
}

/// engine-cache-link: a project whose local release engine is a link to the shared cache ships a
/// real file. This binary stands in for the project engine: the build only copies it.
#[cfg(unix)]
#[test]
fn a_linked_engine_ships_as_a_real_file() {
    use bevy_ts_cli::engine::{TARGET, sha256_file};
    let project = common::project("build-linked", &[("tsconfig.json", "{}"), ("src/game.ts", "export const x = 1;\n")]);
    let cache = project.join("shared");
    common::write(&project, "bevy-ts.json", &format!(r#"{{ "native": {{ "ports": ["rapier3d"] }}, "engineCache": {:?} }}"#, cache.to_str().unwrap()));
    let out = project.join("dist");
    let run = || {
        let out = Command::new(BIN).arg("build").arg(&project).args(["--json", "--out"]).arg(&out).env("CARGO", "false").output().unwrap();
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    // No toolchain and an empty cache: the build fails, naming the engine hash.
    let first = run();
    let at = first.find("; engine ").expect(&first) + "; engine ".len();
    let hash = &first[at..at + 32];
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "{first}");
    let entry = cache.join(hash).join(TARGET);
    std::fs::create_dir_all(&entry).unwrap();
    std::fs::copy(BIN, entry.join("bevy-ts")).unwrap();
    std::fs::write(entry.join("bevy-ts.sha256"), sha256_file(Path::new(BIN)).unwrap()).unwrap();
    let second = run();
    let done: Value = second.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find(|e| e["event"] == "done").expect(&second);
    let local = project.join(".bevy-ts/engines").join(hash).join("bevy-ts");
    assert!(std::fs::symlink_metadata(&local).unwrap().file_type().is_symlink());
    let shipped = PathBuf::from(done["run"].as_str().unwrap());
    let meta = std::fs::symlink_metadata(&shipped).unwrap();
    assert!(meta.file_type().is_file(), "{}", shipped.display());
    assert_eq!(meta.len(), std::fs::metadata(BIN).unwrap().len());
}

/// A native module (`bevy-ts/input`, `bevy-ts/assets`, `bevy-ts/camera`) registers while the
/// engine's app builds, and the build compiles the scripts first: it must build that app before it
/// resolves imports. The shipped game runs their values.
#[test]
fn a_build_imports_the_native_modules() {
    const GAME: &str = r#"import { MouseButton } from "bevy-ts/input";
import { Mesh, PrimitiveTopology } from "bevy-ts/assets";
import type { Cameras } from "bevy-ts/camera";
export const a: Cameras | number = 1;
console.log(`modules ${MouseButton !== undefined} ${Mesh.new(PrimitiveTopology.TriangleList).countVertices()}`);
"#;
    let project = common::project("build-native-modules", &[("tsconfig.json", "{}"), ("src/game.ts", GAME)]);
    let out = project.join("dist/here");
    let (code, events) = build(&[project.to_str().unwrap(), "--json", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{events:#?}");
    let (engine, rx) = play(&out.join("game"));
    wait_for(&rx, &["modules true 0", "generation 1 applied"], LAUNCH);
    drop(engine);
}

/// Every module `types` declares is one `build` resolves, so a new module cannot regress this.
#[test]
fn a_build_imports_every_module_types_lists() {
    let project = common::project("build-every-module", &[("tsconfig.json", "{}"), ("src/game.ts", "export const a = 1;\n")]);
    let dts = project.join("native.d.ts");
    let st = Command::new(BIN).arg("types").arg("--out").arg(&dts).current_dir(&project).status().unwrap();
    assert!(st.success());
    let text = std::fs::read_to_string(project.join("native-modules.d.ts")).unwrap();
    // A module with a value export has JS to import; one of only types (`bevy-ts/camera`) is imported
    // with `import type`, which the compile erases.
    let blocks: Vec<&str> = text.split("declare module \"").skip(1).collect();
    let modules: Vec<&str> = blocks.iter().filter(|b| b.contains("\n  export const ") || b.contains("\n  export function ") || b.contains("\n  export enum ")).filter_map(|b| b.split('"').next()).collect();
    assert!(modules.contains(&"bevy-ts/input"), "{modules:?}");
    let src: String = modules.iter().map(|m| format!("import \"{m}\";\n")).collect::<String>() + "export const a = 1;\n";
    common::write(&project, "src/game.ts", &src);
    let out = project.join("dist/here");
    let (code, events) = build(&[project.to_str().unwrap(), "--json", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{modules:?}\n{events:#?}");
}

/// A port's module (`bevy-ts/rapier`) registers only in the project engine, so a project with Rust
/// builds through it.
#[test]
#[ignore = "builds the rapier project engine"]
fn a_port_project_builds_with_its_module() {
    let rapier = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/rapier");
    let scratch = common::TestDir::new("build-rapier");
    let out = scratch.join("rapier");
    let (code, events) = build(&[rapier.to_str().unwrap(), "--json", "--out", out.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{events:#?}");
}
