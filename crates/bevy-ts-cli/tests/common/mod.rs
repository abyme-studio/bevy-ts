#![allow(dead_code)]

use bevy::prelude::*;
use bevy_ts::generation::{DevSource, ReloadReport};
use bevy_ts::log::{LogEntry, ScriptLog};
use bevy_ts::BevyTsPlugin;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_v8::V8Options;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A fresh directory of TS files under the temp dir.
pub fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join("bevy-ts-cli-tests").join(name);
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
