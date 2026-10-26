//! Script materials on the GPU: a broken `.wgsl` save is reported at the author's line and the
//! shader that draws stays; an error in the generated params names the schema. macOS (Metal).
#![cfg(target_os = "macos")]

use crate::common;

use bevy::prelude::*;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::BevyTsPlugin;
use bevy_ts::generation::{DevChannel, DevEvent, DevSource, ReloadStatus};
use bevy_ts::log::LogLevel;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_material::wgsl::PipelineWatch;
use bevy_ts_material::{MaterialKinds, ScriptMaterialPlugin};
use common::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// Line 12 is `return vec4<f32>(params.color.rgb * glow, params.color.a);`.
const WGSL: &str = "// game::Glow's fragment shader.
import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::globals;

const FLOOR: f32 = 0.25;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let wave = 0.5 + 0.5 * sin(globals.time * params.pulse);
    let glow = FLOOR + (1.0 - FLOOR) * wave;
    let uv = in.uv;
    return vec4<f32>(params.color.rgb * glow, params.color.a);
}
";

fn game(params: &str) -> String {
    game_with(params, "")
}

/// `extra` goes into material()'s options (`, textures: { noise: texture2d }`, `, vertex: vert`).
fn game_with(params: &str, extra: &str) -> String {
    let vert = if extra.contains("vertex") { "import vert from \"./vert.wgsl\";\n" } else { "" };
    format!(
        "import {{ app, Startup, Transform }} from \"bevy-ts\";\nimport type {{ Commands }} from \"bevy-ts\";\nimport {{ Camera3d }} from \"bevy-ts/bevy\";\nimport {{ color, f32, material, texture2d }} from \"bevy-ts/material\";\nimport glow from \"./glow.wgsl\";\n{vert}\nexport const Glow = material(\"game::Glow\", {{ params: {{ {params} }}, fragment: glow{extra} }});\n\nexport function camera(commands: Commands) {{\n  commands.spawn(Camera3d, Transform.lookingAt([0, 0, 4], [0, 0, 0]));\n}}\napp.addSystems(Startup, camera);\n\napp.scene(\"s\", () => (\n  <entity name=\"cube\">\n    <mesh shape=\"cuboid\" />\n    <material kind={{Glow}} color=\"#ff8800\" pulse={{2}} />\n  </entity>\n));\n"
    )
}

fn gpu_app(entry: &Path) -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(bevy_ts_cli::headless::HeadlessPlugin)
    .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin)
    .add_plugins(ScriptMaterialPlugin);
    app.insert_resource(DevSource(Arc::new(Compiler::new(entry, true).unwrap())));
    app.finish();
    app.cleanup();
    app
}

fn fragment(app: &App) -> (bevy::asset::AssetId<Shader>, Option<String>) {
    let s = &app.world().resource::<MaterialKinds>().live["game::Glow"].fragment;
    (s.handle.id(), s.source.clone())
}

/// Updates until a pipeline built from the fragment shader compiled.
fn until_drawn(app: &mut App) {
    let (id, _) = fragment(app);
    until(app, "the material's pipeline never compiled", |app| app.world().resource::<PipelineWatch>().0.lock().unwrap().defs.contains_key(&id));
}

fn errors(app: &App) -> Vec<String> {
    logs(app).into_iter().filter(|e| e.level == LogLevel::Error).map(|e| format!("{} @ {:?}", e.message, e.location)).collect()
}

fn setup(name: &str) -> (PathBuf, App) {
    let dir = project(name, &[("game.tsx", &game("color: color, pulse: f32")), ("glow.wgsl", WGSL)]);
    let mut app = gpu_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    until_drawn(&mut app);
    (dir, app)
}

fn save_wgsl(app: &mut App, dir: &Path, src: &str) {
    let path = dir.join("glow.wgsl").canonicalize().unwrap();
    std::fs::write(&path, src).unwrap();
    let (tx, channel) = DevChannel::new();
    app.insert_resource(channel);
    tx.send(DevEvent::FilesChanged(vec![path])).unwrap();
    app.update();
}

fn error_at(app: &App, line: u32) -> String {
    let e = logs(app).into_iter().rev().find(|e| e.level == LogLevel::Error).unwrap_or_else(|| panic!("no error logged"));
    let at = e.location.clone().unwrap_or_else(|| panic!("no location: {}", e.message));
    assert!(e.message.contains("game::Glow"), "{}", e.message);
    assert!(at.file.ends_with("glow.wgsl"), "{at:?}: {}", e.message);
    assert_eq!(at.line, line, "{}", e.message);
    e.message
}

#[test]
fn a_type_error_names_the_authors_line_and_the_old_shader_stays() {
    let (dir, mut app) = setup("material-errors-type");
    let before = fragment(&app);
    save_wgsl(&mut app, &dir, &WGSL.replace("params.color.rgb * glow, params.color.a", "params.color.rgb * glow, 1u"));
    let m = error_at(&app, 12);
    assert!(m.contains("previous shader keeps drawing"), "{m}");
    assert_eq!(fragment(&app), before, "the shader asset is untouched");
    // Fixed again: replaced, and no error, not even a stale one from the renderer.
    let n = errors(&app).len();
    save_wgsl(&mut app, &dir, &WGSL.replace("0.25", "0.5"));
    updates(&mut app, 120);
    assert_eq!(errors(&app).len(), n, "{:?}", errors(&app));
    assert!(fragment(&app).1.unwrap().contains("const FLOOR: f32 = 0.5;"));
}

#[test]
fn a_rejected_schema_change_is_retried_when_the_wgsl_is_fixed() {
    let (dir, mut app) = setup("material-errors-retry");
    let before = fragment(&app);
    // pulse renamed to speed in TS while glow.wgsl still reads params.pulse: rejected, kept.
    write(&dir, "game.tsx", &game("color: color, speed: f32").replace("pulse={2}", "speed={3}"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let m = error_at(&app, 9);
    assert!(m.contains("previous material keeps drawing"), "{m}");
    assert_eq!(fragment(&app), before);
    // The shader fixed, with no TS touched: the newer schema applies.
    save_wgsl(&mut app, &dir, &WGSL.replace("params.pulse", "params.speed"));
    let kinds = app.world().resource::<MaterialKinds>();
    let k = &kinds.live["game::Glow"];
    assert!(k.wanted.is_none(), "applied");
    assert!(k.decl.params.iter().any(|p| p.name == "speed"));
    assert!(fragment(&app).1.unwrap().contains("    speed: f32,\n"));
    let world = app.world_mut();
    let h = world.query::<&bevy_ts_material::ScriptMaterial3d>().single(world).unwrap().0.clone();
    let m = app.world().resource::<Assets<bevy_ts_material::ScriptMaterial>>().get(&h).unwrap();
    assert_eq!(f32::from_le_bytes(m.data[16..20].try_into().unwrap()), 3.0, "speed re-encoded from the props");
}

#[test]
fn a_param_the_schema_lacks_names_its_line() {
    let (dir, mut app) = setup("material-errors-field");
    let before = fragment(&app);
    save_wgsl(&mut app, &dir, &WGSL.replace("sin(globals.time * params.pulse)", "sin(globals.time * params.speed)"));
    let m = error_at(&app, 9);
    assert!(m.contains("speed"), "{m}");
    assert_eq!(fragment(&app), before);
}

#[test]
fn an_error_in_the_generated_params_names_the_schema() {
    let (dir, mut app) = setup("material-errors-generated");
    // The file already declares `noise`; a texture of that name makes the generated code declare
    // it again.
    save_wgsl(&mut app, &dir, &WGSL.replace("const FLOOR: f32 = 0.25;", "const FLOOR: f32 = 0.25;\nconst noise: f32 = 1.0;"));
    // Keeps-drawing is said only once a pipeline built from the saved shader draws.
    let (id, _) = fragment(&app);
    until(&mut app, "the saved shader never drew", |app| app.world().resource::<PipelineWatch>().0.lock().unwrap().drawing.contains(&id));
    let before = fragment(&app);
    write(&dir, "game.tsx", &game_with("color: color, pulse: f32", ", textures: { noise: texture2d }"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let e = logs(&app).into_iter().rev().find(|e| e.level == LogLevel::Error).expect("an error");
    assert!(e.message.contains("game::Glow") && e.message.contains("`noise`") && e.message.contains("params, textures or instance params generate"), "{}", e.message);
    assert!(e.location.clone().expect("the material() call").file.ends_with("game.tsx"));
    assert!(e.message.contains("previous material keeps drawing"), "{}", e.message);
    assert_eq!(fragment(&app), before, "the old shader stays");
}

#[test]
fn a_broken_first_shader_is_reported_by_the_renderer_at_its_line() {
    let broken = WGSL.replace("sin(globals.time * params.pulse)", "sin(globals.time * params.speed)");
    let dir = project("material-errors-first", &[("game.tsx", &game("color: color, pulse: f32")), ("glow.wgsl", &broken)]);
    let mut app = gpu_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    until(&mut app, "no error was logged", |app| !errors(app).is_empty());
    let m = error_at(&app, 9);
    assert!(!m.contains("keeps drawing"), "nothing drew, so nothing is kept: {m}");
    // A second broken save, with no drawing shader to check it against first: the renderer's
    // failure is reported too.
    let n = errors(&app).len();
    save_wgsl(&mut app, &dir, &WGSL.replace("params.color.rgb * glow", "params.colour.rgb * glow"));
    until_error(&mut app, n);
    let m = error_at(&app, 12);
    assert!(m.contains("colour"), "{m}");
}

#[test]
fn adding_a_broken_vertex_shader_keeps_the_material_drawing() {
    let (dir, mut app) = setup("material-errors-vertex");
    let before = fragment(&app);
    write(&dir, "vert.wgsl", "import bevy_pbr::render::forward_io::VertexOutput;\n\n@vertex\nfn vertex() -> VertexOutput {\n    return 1.0;\n}\n");
    write(&dir, "game.tsx", &game_with("color: color, pulse: f32", ", vertex: vert"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let e = logs(&app).into_iter().rev().find(|e| e.level == LogLevel::Error).expect("an error");
    let at = e.location.clone().expect("a location");
    assert!(at.file.ends_with("vert.wgsl") && e.message.contains("previous material keeps drawing"), "{} @ {at:?}", e.message);
    assert_eq!(at.line, 5, "{}", e.message);
    let kinds = app.world().resource::<MaterialKinds>();
    let k = &kinds.live["game::Glow"];
    assert!(k.vertex.is_none() && k.wanted.is_some(), "kept as it was");
    assert_eq!(fragment(&app), before);
}

#[test]
fn a_save_the_live_schema_accepts_applies_while_a_newer_one_waits() {
    let (dir, mut app) = setup("material-errors-fallback");
    write(&dir, "game.tsx", &game("color: color, speed: f32").replace("pulse={2}", "speed={3}"));
    reload(&mut app, false);
    error_at(&app, 9);
    // Valid only under the live schema (it still reads params.pulse): the live shader takes it.
    save_wgsl(&mut app, &dir, &WGSL.replace("0.25", "0.75"));
    let k = &app.world().resource::<MaterialKinds>().live["game::Glow"];
    assert!(k.wanted.is_some(), "the newer schema still waits");
    assert!(fragment(&app).1.unwrap().contains("const FLOOR: f32 = 0.75;"));
}

/// Updates until an error past the first `n` is logged.
fn until_error(app: &mut App, n: usize) {
    until(app, &format!("no error past the first {n}"), |app| errors(app).len() > n);
}

// Metal has no f64 (every other capability the check assumes): macOS only.
#[cfg(target_os = "macos")]
#[test]
fn a_failure_only_the_device_sees_is_reported_with_the_renderers_message() {
    let (dir, mut app) = setup("material-errors-device");
    let n = errors(&app).len();
    // Valid WGSL to the check (every capability), but Metal has no f64.
    save_wgsl(&mut app, &dir, &WGSL.replace("let uv = in.uv;", "let uv = in.uv;\n    let wide: f64 = 1.0lf;"));
    until_error(&mut app, n);
    let e = errors(&app);
    assert!(e.len() > n, "the renderer's failure is reported");
    let last = e.last().unwrap();
    assert!(last.contains("game::Glow") && last.contains("renderer could not build"), "{last}");
}

#[test]
fn a_stale_failure_is_not_reported_against_a_fixed_file_the_check_skips() {
    let (dir, mut app) = setup("material-errors-stale");
    // `@if` outside the imports: the check skips the file, so only the renderer sees it.
    let unchecked = WGSL.replace("const FLOOR: f32 = 0.25;", "const FLOOR: f32 = 0.25;\n@if(BEVY_TS_NEVER) const UNUSED: f32 = 1.0;");
    let n = errors(&app).len();
    save_wgsl(&mut app, &dir, &unchecked.replace("params.color.rgb * glow, params.color.a", "params.color.rgb * glow, 1u"));
    until_error(&mut app, n);
    assert!(errors(&app).len() > n, "the broken file is reported");
    // Fixed (still unchecked): its pipelines compile, and the old failure is not told again.
    let n = errors(&app).len();
    save_wgsl(&mut app, &dir, &unchecked);
    updates(&mut app, 240);
    assert_eq!(errors(&app).len(), n, "{:?}", &errors(&app)[n..]);
}

#[test]
fn nothing_is_said_to_keep_drawing_after_the_renderer_rejected_the_last_shader() {
    let (dir, mut app) = setup("material-errors-not-drawing");
    // A file the check skips, broken: installed, the renderer rejects it, nothing draws.
    let unchecked = WGSL.replace("const FLOOR: f32 = 0.25;", "const FLOOR: f32 = 0.25;\n@if(BEVY_TS_NEVER) const UNUSED: f32 = 1.0;");
    let n = errors(&app).len();
    save_wgsl(&mut app, &dir, &unchecked.replace("params.color.rgb * glow, params.color.a", "params.color.rgb * glow, 1u"));
    until_error(&mut app, n);
    updates(&mut app, 10);
    // A checkable broken save now: its error must not claim a previous shader draws.
    save_wgsl(&mut app, &dir, &WGSL.replace("sin(globals.time * params.pulse)", "sin(globals.time * params.speed)"));
    let m = error_at(&app, 9);
    assert!(!m.contains("keeps drawing"), "{m}");
}
