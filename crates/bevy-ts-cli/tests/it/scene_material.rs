//! A script material on a glTF scene root, end to end (change `scene-material`): `material()` in
//! V8, `<WorldAssetRoot>` and `<material kind gltfMaterial>` in TSX, the scene loaded by Bevy's
//! glTF loader. Headless: no window, no GPU.

use crate::common;

use bevy::gltf::GltfMaterialName;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::generation::ReloadStatus;
use bevy_ts_material::{FadeMaterial, ScriptMaterial3d, ScriptMaterialPlugin};
use bevy_ts_v8::V8Options;
use common::*;
use std::path::Path;
use std::sync::Arc;

const WGSL: &str = "import bevy_pbr::render::forward_io::VertexOutput;\n\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {\n    return params.color * params.pulse;\n}\n";

fn game(element: &str) -> String {
    format!(
        "import {{ app }} from \"bevy-ts\";\nimport {{ WorldAssetRoot }} from \"bevy-ts/bevy\";\nimport {{ color, f32, material }} from \"bevy-ts/material\";\nimport glow from \"./glow.wgsl\";\n\nexport const Glow = material(\"game::Glow\", {{ params: {{ color: color, pulse: f32 }}, fragment: glow }});\n\napp.scene(\"s\", () => (\n  <entity name=\"ship\">\n    <WorldAssetRoot {{...{{ \"0\": \"models/ship.gltf#Scene0\" }}}} />\n    {element}\n  </entity>\n));\n"
    )
}

fn scene_app(dir: &Path) -> App {
    let assets = dir.join("assets").canonicalize().unwrap();
    let render = RenderPlugin { render_creation: RenderCreation::Automatic(Box::new(WgpuSettings { backends: None, ..default() })), ..default() };
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(render)
            .set(AssetPlugin { file_path: assets.to_string_lossy().into_owned(), watch_for_changes_override: Some(false), ..default() }),
    )
    .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin)
    .add_plugins(ScriptMaterialPlugin);
    app.insert_resource(bevy_ts::generation::DevSource(Arc::new(bevy_ts_cli::compile::Compiler::new(dir.join("src/game.tsx"), true).unwrap())));
    app.finish();
    app.cleanup();
    app
}

fn setup(name: &str, element: &str) -> (std::path::PathBuf, App) {
    let ship = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../bevy-ts-material/tests/assets/models/ship.gltf")).unwrap();
    let dir = project(name, &[("src/game.tsx", &game(element)), ("src/glow.wgsl", WGSL), ("assets/models/ship.gltf", &ship)]);
    let mut app = scene_app(&dir);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    (dir, app)
}

/// The scene's meshes, by glTF material name: (holds a script material, keeps its own).
fn by_name(app: &mut App) -> Vec<(String, bool, bool)> {
    let world = app.world_mut();
    let mut out: Vec<_> = world
        .query::<(&GltfMaterialName, Option<&ScriptMaterial3d>, Option<&MeshMaterial3d<StandardMaterial>>)>()
        .iter(world)
        .map(|(n, s, m)| (n.0.clone(), s.is_some(), m.is_some()))
        .collect();
    out.sort();
    out
}

fn hull(on: bool) -> (String, bool, bool) {
    ("hull".into(), on, !on)
}

fn trim(on: bool) -> (String, bool, bool) {
    ("trim".into(), on, !on)
}

#[test]
fn a_scene_mesh_carries_the_script_material() {
    let (dir, mut app) = setup("scene-material", "<material kind={Glow} gltfMaterial=\"hull\" pulse={2} />");
    until(&mut app, "the hull swapped", |app| by_name(app) == vec![hull(true), hull(true), trim(false)]);
    let errors: Vec<String> = logs(&app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message).collect();
    assert!(errors.is_empty(), "{errors:?}");
    // New props: the hull meshes take the new instance.
    let before = {
        let world = app.world_mut();
        world.query_filtered::<&ScriptMaterial3d, With<GltfMaterialName>>().iter(world).next().unwrap().clone()
    };
    write(&dir, "src/game.tsx", &game("<material kind={Glow} gltfMaterial=\"hull\" pulse={3} />"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    updates(&mut app, 2);
    let world = app.world_mut();
    let now: Vec<ScriptMaterial3d> = world.query_filtered::<&ScriptMaterial3d, With<GltfMaterialName>>().iter(world).cloned().collect();
    assert_eq!(now.len(), 2);
    assert!(now.iter().all(|m| *m != before), "the new instance on every hull mesh");
    // No filter: every mesh.
    write(&dir, "src/game.tsx", &game("<material kind={Glow} pulse={3} />"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    updates(&mut app, 2);
    assert_eq!(by_name(&mut app), vec![hull(true), hull(true), trim(true)]);
    // No material: the scene's own back.
    write(&dir, "src/game.tsx", &game(""));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    updates(&mut app, 2);
    assert_eq!(by_name(&mut app), vec![hull(false), hull(false), trim(false)]);
}

#[test]
fn a_reserved_param_name_fails_at_material() {
    let src = "import { app } from \"bevy-ts\";\nimport { f32, material } from \"bevy-ts/material\";\nimport glow from \"./glow.wgsl\";\n\nexport const M = material(\"game::M\", { params: { gltfMaterial: f32 }, fragment: glow });\n";
    let dir = project("scene-material-reserved", &[("src/game.tsx", src), ("src/glow.wgsl", WGSL), ("assets/.keep", "")]);
    let mut app = scene_app(&dir);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = &r.errors[0];
    assert!(e.message.contains("<material> takes gltfMaterial itself"), "{e:?}");
    assert_eq!(e.location.clone().expect("a location").line, 5);
}

// Change material-alpha: `<MaterialAlpha>` bound on a scene root.

#[test]
fn a_bound_material_alpha_fades_a_scene_frame_by_frame() {
    let fade = "<MaterialAlpha alpha={() => k()} />";
    let src = game(fade)
        .replace("import { WorldAssetRoot } from \"bevy-ts/bevy\";", "import { MaterialAlpha, WorldAssetRoot } from \"bevy-ts/bevy\";\nimport { signal, Update, type Local, type u32 } from \"bevy-ts\";")
        .replace("\napp.scene(", "\nexport const [k, setK] = signal(1);\nexport function fade(frames: Local<{ n: u32 }>) { frames.n += 1; setK(Math.max(0, 1 - frames.n / 20)); }\napp.addSystems(Update, fade);\n\napp.scene(");
    let ship = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../bevy-ts-material/tests/assets/models/ship.gltf")).unwrap();
    let dir = project("scene-material-alpha", &[("src/game.tsx", &src), ("src/glow.wgsl", WGSL), ("assets/models/ship.gltf", &ship)]);
    let mut app = scene_app(&dir);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let alphas = |app: &mut App| {
        let world = app.world_mut();
        let hs: Vec<Handle<FadeMaterial>> = world.query_filtered::<&MeshMaterial3d<FadeMaterial>, With<GltfMaterialName>>().iter(world).map(|m| m.0.clone()).collect();
        let assets = world.resource::<Assets<FadeMaterial>>();
        hs.iter().map(|h| assets.get(h).map(|m| (m.extension.alpha, m.base.alpha_mode))).collect::<Option<Vec<_>>>().unwrap_or_default()
    };
    until(&mut app, "the ship faded", |app| {
        let a = alphas(app);
        a.len() == 3 && a.iter().all(|(x, m)| *x < 1.0 && *m == AlphaMode::Opaque)
    });
    let count = |app: &App| app.world().resource::<Assets<StandardMaterial>>().len() + app.world().resource::<Assets<FadeMaterial>>().len();
    let assets = count(&app);
    let mut last = f32::MAX;
    for _ in 0..40 {
        app.update();
        let a = alphas(&mut app);
        assert_eq!(count(&app), assets, "no asset added");
        assert!(a.iter().all(|(x, _)| *x == a[0].0), "every mesh at the root's alpha: {a:?}");
        assert!(a[0].0 <= last);
        last = a[0].0;
    }
    assert_eq!(last, 0.0);
}
