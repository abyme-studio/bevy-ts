//! Change `script-packages`: `bevy-ts dev` reloads when a package is installed or its `exports`
//! change, and a dynamic `import()` outside the compiled graph rejects catchably.

mod common;

use bevy::prelude::*;
use bevy_ts::generation::{DevChannel, Generations, ReloadLog, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use common::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// An app whose dev watcher compiles `game.ts` in `dir`.
fn watched(dir: &std::path::Path) -> App {
    let mut app = app(&dir.join("game.ts"));
    let (tx, channel) = DevChannel::new();
    app.insert_resource(channel);
    bevy_ts_cli::dev::spawn_watcher(Arc::new(Compiler::new(dir.join("game.ts"), true).unwrap()), tx);
    app
}

/// Updates until `done` holds, or panics after 10 s naming `what`.
fn until(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    let start = Instant::now();
    while !done(app) {
        assert!(start.elapsed() < Duration::from_secs(10), "{what} never happened: {:?}", messages(app));
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn statuses(app: &App) -> Vec<ReloadStatus> {
    app.world().resource::<ReloadLog>().since(0).map(|r| r.status).collect()
}

#[test]
fn installing_a_missing_package_reloads() {
    let dir = project("install", &[("package.json", r#"{"name": "game", "type": "module"}"#), ("game.ts", "import { pad } from \"lodash-es\";\nconsole.log(pad(\"installed\"));\n")]);
    let mut app = watched(&dir);
    until(&mut app, "a failed load", |app| statuses(app).contains(&ReloadStatus::Failed));
    std::thread::sleep(Duration::from_millis(100));
    // `pnpm add lodash-es`, by hand.
    write(&dir, "node_modules/lodash-es/package.json", r#"{"name": "lodash-es", "type": "module", "exports": "./lodash.js"}"#);
    write(&dir, "node_modules/lodash-es/lodash.js", "export const pad = (s) => `[${s}]`;\n");
    until(&mut app, "the reload after install", |app| app.world().resource::<Generations>().live >= 1);
    until(&mut app, "the package's log", |app| messages(app).iter().any(|m| m == "[installed]"));
}

#[test]
fn editing_a_watched_package_json_reloads() {
    let dir = project(
        "exports-edit",
        &[
            ("package.json", r#"{"name": "game", "type": "module"}"#),
            ("game.ts", "import { which } from \"dep\";\nconsole.log(`dep ${which}`);\n"),
            ("node_modules/dep/package.json", r#"{"name": "dep", "type": "module", "exports": "./a.js"}"#),
            ("node_modules/dep/a.js", "export const which = \"a\";\n"),
            ("node_modules/dep/b.js", "export const which = \"b\";\n"),
        ],
    );
    let mut app = watched(&dir);
    until(&mut app, "dep a", |app| messages(app).iter().any(|m| m == "dep a"));
    std::thread::sleep(Duration::from_millis(100));
    write(&dir, "node_modules/dep/package.json", r#"{"name": "dep", "type": "module", "exports": "./b.js"}"#);
    until(&mut app, "dep b", |app| messages(app).iter().any(|m| m == "dep b"));
}

#[test]
fn a_dynamic_import_outside_the_graph_rejects_catchably() {
    let src = r#"import { app, Update } from "bevy-ts";
import("../plugins/x/package.json", { with: { type: "json" } }).then(
  () => console.log("loaded"),
  (e) => console.log(`caught: ${e.message}`),
);
import("lodash-es").catch((e) => console.log(`caught bare: ${e.message}`));
export function tick() {}
app.addSystems(Update, tick);
"#;
    let (_dir, mut app) = load("dynamic-import", src);
    until(&mut app, "both rejections", |app| {
        let m = messages(app);
        m.iter().any(|l| l.starts_with("caught: ")) && m.iter().any(|l| l.starts_with("caught bare: "))
    });
    let m = messages(&app);
    assert!(!m.iter().any(|l| l == "loaded"), "{m:?}");
    assert!(m.iter().any(|l| l.contains("import it statically")), "{m:?}");
    updates(&mut app, 3);
    assert_eq!(app.world().resource::<Generations>().live, 1, "the game keeps running");
}
