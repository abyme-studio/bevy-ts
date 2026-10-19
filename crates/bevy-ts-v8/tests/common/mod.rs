#![allow(dead_code)]

use bevy::prelude::*;
use bevy_ts::backend::{CompiledModule, ModuleSet};
use bevy_ts::log::{LogEntry, ScriptLog};
use bevy_ts::BevyTsPlugin;
use bevy_ts_v8::V8Options;
use std::path::PathBuf;

/// A headless app with the V8 backend.
pub fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true));
    app
}

/// A module set from (file name, JS source) pairs; the first is the entry.
pub fn set(files: &[(&str, &str)]) -> ModuleSet {
    let dir = std::env::temp_dir().join("bevy-ts-tests");
    let modules: Vec<CompiledModule> = files
        .iter()
        .map(|(name, code)| CompiledModule { path: dir.join(name), code: code.to_string(), source_map: None })
        .collect();
    ModuleSet { entry: modules[0].path.clone(), modules }
}

pub fn entry_path(name: &str) -> PathBuf {
    std::env::temp_dir().join("bevy-ts-tests").join(name)
}

/// Loads JS as the next generation and panics with its errors if it fails.
pub fn load(app: &mut App, files: &[(&str, &str)]) {
    let report = bevy_ts::generation::apply(app.world_mut(), set(files), false);
    assert!(report.errors.is_empty(), "load failed: {:?}", report.errors);
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
