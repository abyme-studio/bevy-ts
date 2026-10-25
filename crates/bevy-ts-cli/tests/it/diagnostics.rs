//! `bevy-ts/diagnostics` in an app with the V8 backend: reading Bevy's store, the frame counter,
//! a script's own diagnostic across reloads, and the paths a script may not take.

use crate::common;

use bevy::diagnostic::{DiagnosticPath, DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use common::*;

fn diag_app(dir: &std::path::Path) -> App {
    let mut app = app(&dir.join("game.ts"));
    app.add_plugins(FrameTimeDiagnosticsPlugin::default()).add_plugins(bevy_ts_cli::diagnostics::ScriptDiagnosticsPlugin);
    app
}

fn loaded(name: &str, src: &str) -> (std::path::PathBuf, App) {
    let dir = project(name, &[("game.ts", src)]);
    let mut app = diag_app(&dir);
    let r = reload(&mut app, false);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    (dir, app)
}

/// The messages that start with `prefix`, with it stripped.
fn tagged(app: &App, prefix: &str) -> Vec<String> {
    messages(app).into_iter().filter_map(|m| m.strip_prefix(prefix).map(str::to_string)).collect()
}

fn store(app: &App) -> &DiagnosticsStore {
    app.world().resource::<DiagnosticsStore>()
}

const READ: &str = r#"import { app, Last, Startup, Update } from "bevy-ts";
import type { DiagnosticsStore, FrameCount } from "bevy-ts/diagnostics";
import { FrameTimeDiagnosticsPlugin } from "bevy-ts/diagnostics";

export function first(frames: FrameCount) {
  console.log(`startup ${frames.value()}`);
}
export function count(frames: FrameCount) {
  console.log(`update ${frames.value()}`);
}
export function read(store: DiagnosticsStore) {
  const paths = [];
  for (let i = 0; i < store.len(); i++) paths.push(store.path(i));
  console.log(`paths ${paths.join(",")} ${store.path(store.len())}`);
  console.log(`frame_time ${store.value(FrameTimeDiagnosticsPlugin.FRAME_TIME)} ${store.suffix("frame_time")}`);
  console.log(`unknown ${store.value("no/such")} ${store.suffix("no/such")} ${store.historyLen("no/such")}`);
}
app.addSystems(Startup, first);
app.addSystems(Update, count);
app.addSystems(Last, read);
"#;

#[test]
fn ts_reads_the_store_and_the_frame_count_as_rust_does() {
    let (_dir, mut app) = loaded("diagnostics-read", READ);
    updates(&mut app, 4);
    assert_eq!(tagged(&app, "startup ").first().map(String::as_str), Some("0"));
    let counts = tagged(&app, "update ");
    assert_eq!(counts.get(2).map(String::as_str), Some("2"), "the third update reads 2: {counts:?}");
    // Read in `Last`, after FrameTimeDiagnosticsPlugin measured: values Rust's store holds.
    let rust: Vec<String> = store(&app).get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).unwrap().values().map(|v| format!("{v} ms")).collect();
    let ts = tagged(&app, "frame_time ").last().cloned().unwrap();
    assert!(rust.contains(&ts), "TS read {ts}, Rust holds {rust:?}");
    let paths = tagged(&app, "paths ").last().cloned().unwrap();
    assert_eq!(paths, "fps,frame_count,frame_time undefined", "sorted, and past the end is undefined");
    assert_eq!(tagged(&app, "unknown ").last().map(String::as_str), Some("undefined undefined undefined"));
}

fn own(settings: &str, measure: bool) -> String {
    format!(
        r#"import {{ app, Update }} from "bevy-ts";
import type {{ Diagnostics, DiagnosticsStore }} from "bevy-ts/diagnostics";
import {{ Diagnostic }} from "bevy-ts/diagnostics";

app.registerDiagnostic(new Diagnostic("game/budget"){settings});
let n = 0;
export function measure(d: Diagnostics) {{
  if ({measure}) d.addMeasurement("game/budget", ++n);
  d.addMeasurement("game/nobody", 7);
}}
export function read(store: DiagnosticsStore) {{
  const out = new Float64Array(8);
  const k = store.history("game/budget", out);
  console.log(`own ${{store.isEnabled("game/budget")}} ${{store.historyLen("game/budget")}} ${{Array.from(out.subarray(0, k)).join(",")}}`);
}}
app.addSystems(Update, measure);
app.addSystems(Update, read);
"#
    )
}

#[test]
fn a_script_diagnostic_is_measured_and_survives_a_reload() {
    let budget = DiagnosticPath::new("game/budget");
    let (dir, mut app) = loaded("diagnostics-own", &own(".withSuffix(\"ms\").withMaxHistoryLength(4)", true));
    updates(&mut app, 3);
    let d = store(&app).get(&budget).expect("registered");
    assert_eq!((d.suffix.as_ref(), d.get_max_history_length()), ("ms", 4));
    assert_eq!(d.values().copied().collect::<Vec<_>>(), [1.0, 2.0, 3.0], "measured from TS, applied after each run");
    assert!(store(&app).get(&DiagnosticPath::new("game/nobody")).is_none(), "an unregistered path records nothing");
    assert!(tagged(&app, "own ").last().unwrap().starts_with("true "), "TS reads it");

    // The same declaration: history kept.
    write(&dir, "game.ts", &own(".withSuffix(\"ms\").withMaxHistoryLength(4)", false));
    assert!(reload(&mut app, false).errors.is_empty());
    assert_eq!(store(&app).get(&budget).unwrap().history_len(), 3);
    // Other settings: replaced, history gone.
    write(&dir, "game.ts", &own(".withSuffix(\"s\").withMaxHistoryLength(4)", false));
    assert!(reload(&mut app, false).errors.is_empty());
    let d = store(&app).get(&budget).unwrap();
    assert_eq!((d.suffix.as_ref(), d.history_len(), d.is_enabled), ("s", 0, true));
    // No longer declared: disabled, and measurements of it dropped.
    write(&dir, "game.ts", READ);
    assert!(reload(&mut app, false).errors.is_empty());
    assert!(!store(&app).get(&budget).unwrap().is_enabled);
}

#[test]
fn a_path_a_rust_plugin_registered_fails_the_load() {
    let src = r#"import { app } from "bevy-ts";
import { Diagnostic, FrameTimeDiagnosticsPlugin } from "bevy-ts/diagnostics";
app.registerDiagnostic(new Diagnostic(FrameTimeDiagnosticsPlugin.FPS));
"#;
    let dir = project("diagnostics-rust-owned", &[("game.ts", src)]);
    let mut app = diag_app(&dir);
    let r = reload(&mut app, false);
    assert_eq!(r.status, bevy_ts::generation::ReloadStatus::Failed);
    let e = &r.errors[0];
    assert!(e.message.contains("registerDiagnostic(\"fps\")") && e.message.contains("Rust plugin"), "{e:?}");
    assert_eq!(e.location.as_ref().map(|l| l.line), Some(3), "located at the call: {e:?}");
}

fn engine(diagnostics: &[&str]) -> App {
    use bevy_ts_cli::cli::{Display, Engine, base_app};
    let engine = Engine { identity: bevy_ts_cli::EngineIdentity::prebuilt(env!("CARGO_PKG_VERSION"), "test") };
    let names: Vec<String> = diagnostics.iter().map(|s| s.to_string()).collect();
    let mut app = base_app(&engine, bevy_ts_cli::Prebuilt, Display::Headless, Default::default(), None, true, false, &names, false);
    app.finish();
    app.cleanup();
    app
}

#[test]
fn an_engine_adds_only_the_plugins_listed() {
    let paths = |app: &App| {
        let mut v: Vec<String> = store(app).iter().map(|d| d.path().to_string()).collect();
        v.sort();
        v
    };
    // Bevy's AssetPlugin registers `started_load_count` in every app; nothing else is there.
    let none = paths(&engine(&[]));
    assert_eq!(none, ["started_load_count"], "off by default");
    // SystemInformationDiagnosticsPlugin is accepted, but Bevy compiles it to nothing under
    // `dynamic_linking`, which every test build has: it adds no path here.
    let app = engine(&["EntityCountDiagnosticsPlugin", "SystemInformationDiagnosticsPlugin"]);
    assert_eq!(paths(&app), ["entity_count", "started_load_count"]);
}

#[test]
fn script_stage_diagnostics_time_stages_with_systems_only() {
    let src = r#"import { app, Last, Update } from "bevy-ts";
import type { DiagnosticsStore } from "bevy-ts/diagnostics";
import { ScriptStageDiagnosticsPlugin } from "bevy-ts/diagnostics";
export function busy() {
  const t = performance.now();
  while (performance.now() - t < 2) {}
}
app.addSystems(Update, busy);
export function read(store: DiagnosticsStore) {
  console.log(`stage ${ScriptStageDiagnosticsPlugin.elapsedCpu("FixedPostUpdate")} ${store.historyLen(ScriptStageDiagnosticsPlugin.elapsedCpu("Update"))}`);
}
app.addSystems(Last, read);
"#;
    let dir = project("diagnostics-stages", &[("game.ts", src)]);
    let mut app = diag_app(&dir);
    app.add_plugins(bevy_ts::systems::ScriptStageDiagnosticsPlugin);
    assert!(reload(&mut app, false).errors.is_empty());
    updates(&mut app, 3);
    let path = bevy_ts::systems::stage_path(bevy_ts::manifest::Stage::Update);
    assert_eq!(path.as_str(), "bevy_ts/update/elapsed_cpu");
    assert_eq!(bevy_ts::systems::stage_path(bevy_ts::manifest::Stage::FixedPostUpdate).as_str(), "bevy_ts/fixed_post_update/elapsed_cpu");
    let update = store(&app).get(&path).expect("Update is timed");
    assert!(update.average().unwrap() >= 2.0, "a 2 ms system: {:?}", update.average());
    assert_eq!(update.suffix, "ms");
    let post = bevy_ts::systems::stage_path(bevy_ts::manifest::Stage::PostUpdate);
    assert!(store(&app).get(&post).is_none(), "a stage with no TS systems records nothing");
    let read = tagged(&app, "stage ").last().cloned().unwrap();
    assert!(read.starts_with("bevy_ts/fixed_post_update/elapsed_cpu ") && !read.ends_with("undefined"), "TS names the path Rust records: {read}");
}
