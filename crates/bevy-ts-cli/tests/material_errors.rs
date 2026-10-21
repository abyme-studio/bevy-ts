//! Script materials on the GPU: a broken `.wgsl` save is reported at the author's line and the
//! shader that draws stays; an error in the generated params names the schema. macOS (Metal).
#![cfg(target_os = "macos")]

mod common;

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
    format!(
        "import {{ app, Startup }} from \"bevy-ts\";\nimport type {{ Commands }} from \"bevy-ts\";\nimport {{ color, f32, material }} from \"bevy-ts/material\";\nimport glow from \"./glow.wgsl\";\n\nexport const Glow = material(\"game::Glow\", {{ params: {{ {params} }}, fragment: glow }});\n\nexport function camera(commands: Commands) {{\n  commands.spawnNative(\"camera3d\", {{ position: [0, 0, 4], lookAt: [0, 0, 0] }});\n}}\napp.addSystems(Startup, camera);\n\napp.scene(\"s\", () => (\n  <entity name=\"cube\">\n    <mesh shape=\"cuboid\" />\n    <material kind={{Glow}} color=\"#ff8800\" pulse={{2}} />\n  </entity>\n));\n"
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
    for _ in 0..600 {
        app.update();
        if app.world().resource::<PipelineWatch>().0.lock().unwrap().defs.contains_key(&id) {
            return;
        }
    }
    panic!("the material's pipeline never compiled; errors: {:?}", errors(app));
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
    // Fixed again: replaced, no error.
    let n = errors(&app).len();
    save_wgsl(&mut app, &dir, &WGSL.replace("0.25", "0.5"));
    assert_eq!(errors(&app).len(), n, "{:?}", errors(&app));
    assert!(fragment(&app).1.unwrap().contains("const FLOOR: f32 = 0.5;"));
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
    let before = fragment(&app);
    // `loop` is a WGSL keyword: the generated struct cannot declare it.
    write(&dir, "game.tsx", &game("color: color, pulse: f32, loop: f32"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let e = logs(&app).into_iter().rev().find(|e| e.level == LogLevel::Error).expect("an error");
    assert!(e.message.contains("game::Glow") && e.message.contains("generated from its params"), "{}", e.message);
    assert!(e.location.expect("the material's location").file.ends_with("game.tsx"));
    assert_eq!(fragment(&app).1, before.1, "the old shader source stays");
}

#[test]
fn a_broken_first_shader_is_reported_by_the_renderer_at_its_line() {
    let broken = WGSL.replace("sin(globals.time * params.pulse)", "sin(globals.time * params.speed)");
    let dir = project("material-errors-first", &[("game.tsx", &game("color: color, pulse: f32")), ("glow.wgsl", &broken)]);
    let mut app = gpu_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    for _ in 0..600 {
        app.update();
        if !errors(&app).is_empty() {
            break;
        }
    }
    error_at(&app, 9);
}
