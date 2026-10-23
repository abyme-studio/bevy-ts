//! Script materials end to end: a `.wgsl` import, `material()` in V8, `<material kind>` in TSX,
//! and the Rust kinds, across reloads and shader saves. No GPU: shader and material assets only.

use crate::common;

use bevy::prelude::*;
use bevy_ts::generation::{DevChannel, DevEvent, ReloadStatus};
use bevy_ts_material::{MaterialKinds, ScriptMaterial, ScriptMaterial3d, ScriptMaterialPlugin};
use common::*;
use std::path::Path;

const WGSL: &str = "import bevy_pbr::render::forward_io::VertexOutput;\n\nconst FLOOR: f32 = 0.25;\n\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {\n    return params.color * FLOOR;\n}\n";

fn game(params: &str, props: &str) -> String {
    format!(
        "import {{ app }} from \"bevy-ts\";\nimport {{ color, f32, material }} from \"bevy-ts/material\";\nimport glow from \"./glow.wgsl\";\n\nexport const Glow = material(\"game::Glow\", {{ params: {{ {params} }}, fragment: glow }});\n\napp.scene(\"s\", () => (\n  <entity name=\"cube\">\n    <mesh shape=\"cuboid\" />\n    <material kind={{Glow}} {props} />\n  </entity>\n));\n"
    )
}

fn material_app(entry: &Path) -> App {
    let mut app = app(entry);
    app.add_plugins(AssetPlugin::default()).init_asset::<Mesh>().init_asset::<StandardMaterial>().init_asset::<Shader>().init_asset::<Image>();
    app.add_plugins(ScriptMaterialPlugin);
    app
}

fn setup(name: &str, src: &str) -> (std::path::PathBuf, App) {
    let dir = project(name, &[("game.tsx", src), ("glow.wgsl", WGSL)]);
    let mut app = material_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    (dir, app)
}

fn cube(app: &mut App) -> (Entity, Handle<ScriptMaterial>) {
    let world = app.world_mut();
    let (e, m) = world.query::<(Entity, &ScriptMaterial3d)>().single(world).expect("one script-material entity");
    (e, m.0.clone())
}

fn f32_at(app: &App, h: &Handle<ScriptMaterial>, offset: usize) -> f32 {
    let m = app.world().resource::<Assets<ScriptMaterial>>().get(h).unwrap();
    f32::from_le_bytes(m.data[offset..offset + 4].try_into().unwrap())
}

fn fragment_source(app: &App) -> String {
    let kinds = app.world().resource::<MaterialKinds>();
    let h = kinds.live["game::Glow"].fragment.handle.clone();
    match &app.world().resource::<Assets<Shader>>().get(&h).unwrap().source {
        bevy::shader::Source::Wesl(s) => s.to_string(),
        other => panic!("not WESL: {other:?}"),
    }
}

#[test]
fn a_material_draws_with_its_file_and_values() {
    let (_dir, mut app) = setup("materials-load", &game("color: color, pulse: f32", "color=\"#000000\" pulse={2}"));
    let (_, h) = cube(&mut app);
    let m = app.world().resource::<Assets<ScriptMaterial>>().get(&h).unwrap();
    assert_eq!(m.data.len(), 32, "GlowParams is 32 bytes");
    assert_eq!(m.kind.name, "game::Glow");
    assert_eq!(f32_at(&app, &h, 0), 0.0);
    assert_eq!(f32_at(&app, &h, 12), 1.0, "alpha of #000000");
    assert_eq!(f32_at(&app, &h, 16), 2.0, "pulse");
    let src = fragment_source(&app);
    assert!(src.starts_with(WGSL), "the author's lines come first, unchanged");
    assert!(src.contains("struct GlowParams {\n    color: vec4<f32>,\n    pulse: f32,\n}"), "{src}");
    assert!(src.contains("@group(3) @binding(0) var<uniform> params: GlowParams;"), "{src}");
}

#[test]
fn a_wgsl_save_replaces_the_shader_in_place_with_no_generation() {
    let (dir, mut app) = setup("materials-save", &game("color: color, pulse: f32", "pulse={2}"));
    let (e, h) = cube(&mut app);
    let shader = app.world().resource::<MaterialKinds>().live["game::Glow"].fragment.handle.clone();
    let generation = app.world().resource::<bevy_ts::generation::Generations>().live;
    let (tx, channel) = DevChannel::new();
    app.insert_resource(channel);
    let path = dir.join("glow.wgsl").canonicalize().unwrap();
    std::fs::write(&path, WGSL.replace("0.25", "0.5")).unwrap();
    tx.send(DevEvent::FilesChanged(vec![path])).unwrap();
    app.update();
    assert!(fragment_source(&app).contains("const FLOOR: f32 = 0.5;"));
    let kinds = app.world().resource::<MaterialKinds>();
    assert_eq!(kinds.live["game::Glow"].fragment.handle, shader, "the same shader asset, replaced in place");
    assert_eq!(app.world().resource::<bevy_ts::generation::Generations>().live, generation, "no new generation");
    assert_eq!(cube(&mut app), (e, h), "no entity respawned, the same material");
}

#[test]
fn a_schema_change_re_encodes_instances_by_name() {
    let (dir, mut app) = setup("materials-schema", &game("color: color, pulse: f32", "color=\"#000000\" pulse={2}"));
    let (e, h) = cube(&mut app);
    // `speed` added before `pulse`: pulse moves from offset 16 to 20 and keeps its value.
    write(&dir, "game.tsx", &game("color: color, speed: f32, pulse: f32", "color=\"#000000\" pulse={2}"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    assert_eq!(cube(&mut app), (e, h.clone()), "same entity, same instance");
    assert_eq!(f32_at(&app, &h, 16), 0.0, "speed defaults to 0");
    assert_eq!(f32_at(&app, &h, 20), 2.0, "pulse re-encoded at its new offset");
    let m = app.world().resource::<Assets<ScriptMaterial>>().get(&h).unwrap();
    assert!(m.kind.bindings.params().field("speed").is_some());
    assert!(fragment_source(&app).contains("    speed: f32,\n"));
}

#[test]
fn a_wgsl_string_is_rejected_with_the_material_and_line() {
    let src = "import { app } from \"bevy-ts\";\nimport { f32, material } from \"bevy-ts/material\";\n\nexport const Glow = material(\"game::Glow\", { params: { pulse: f32 }, fragment: \"@fragment fn f() {}\" });\n";
    let dir = project("materials-string", &[("game.ts", src)]);
    let mut app = material_app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = &r.errors[0];
    assert!(e.message.contains("game::Glow") && e.message.contains("import"), "{e:?}");
    let at = e.location.clone().expect("a location");
    assert!(at.file.ends_with("game.ts"), "{at:?}");
    assert_eq!(at.line, 4);
}

#[test]
fn an_unknown_param_fails_the_scene_at_its_element() {
    let dir = project("materials-unknown", &[("game.tsx", &game("pulse: f32", "speed={3}")), ("glow.wgsl", WGSL)]);
    let mut app = material_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = &r.errors[0];
    assert!(e.message.contains("no param or texture speed") && e.message.contains("pulse"), "{e:?}");
    assert_eq!(e.location.clone().expect("a location").line, 10);
}

#[test]
fn a_scene_described_before_the_first_material_still_checks() {
    // plain.tsx's app.scene runs first, so the `tsx` section is written before `materials`.
    let plain = "import { app } from \"bevy-ts\";\n\napp.scene(\"plain\", () => <entity name=\"plain\" />);\n";
    let main = format!("import \"./plain.tsx\";\n{}", game("color: color, pulse: f32", "pulse={2}"));
    let dir = project("materials-order", &[("plain.tsx", plain), ("game.tsx", &main), ("glow.wgsl", WGSL)]);
    let mut app = material_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let (e, h) = cube(&mut app);
    // A param added in the same save as its use is accepted.
    write(&dir, "game.tsx", &format!("import \"./plain.tsx\";\n{}", game("color: color, pulse: f32, speed: f32", "pulse={2} speed={3}")));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let (e2, h2) = cube(&mut app);
    assert_eq!(e2, e);
    assert_ne!(h2, h, "new props, a new instance");
    assert_eq!(f32_at(&app, &h2, 20), 3.0, "speed");
}

#[test]
fn a_bound_param_fails_once_with_the_spec_message() {
    let dir = project("materials-bound", &[("game.tsx", &game("pulse: f32", "pulse={() => 2}")), ("glow.wgsl", WGSL)]);
    let mut app = material_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
    assert!(r.errors[0].message.contains("props cannot be bound yet"), "{:?}", r.errors);
}

#[test]
fn a_bad_texture_name_fails_the_load_at_the_material() {
    let src = "import { app } from \"bevy-ts\";\nimport { f32, material, texture2d } from \"bevy-ts/material\";\nimport glow from \"./glow.wgsl\";\n\nexport const Glow = material(\"game::Glow\", { params: { pulse: f32 }, textures: { \"noise-map\": texture2d }, fragment: glow });\n";
    let dir = project("materials-texture-name", &[("game.ts", src), ("glow.wgsl", WGSL)]);
    let mut app = material_app(&dir.join("game.ts"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = &r.errors[0];
    assert!(e.message.contains("game::Glow") && e.message.contains("noise-map"), "{e:?}");
    assert_eq!(e.location.clone().expect("the material() call").line, 5);
}

#[test]
fn a_bad_declaration_after_a_scene_is_reported_at_its_material_call() {
    let plain = "import { app } from \"bevy-ts\";\n\napp.scene(\"plain\", () => <entity name=\"plain\" />);\n";
    // `loop` is reserved: the materials check rejects it at material(); the scene's element is quiet.
    let main = format!("import \"./plain.tsx\";\n{}", game("loop: f32", "loop={2}"));
    let dir = project("materials-order-bad", &[("plain.tsx", plain), ("game.tsx", &main), ("glow.wgsl", WGSL)]);
    let mut app = material_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
    assert!(r.errors[0].message.contains("reserved"), "{:?}", r.errors);
    assert_eq!(r.errors[0].location.clone().expect("material()").line, 6);
}

// Change material-alpha: `<material alpha alphaMode>` and `MaterialAlpha`.

fn named(app: &mut App, name: &str) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &Name)>().iter(world).find(|(_, n)| n.as_str() == name).map(|(e, _)| e).unwrap_or_else(|| panic!("no {name}"))
}

fn standard(app: &mut App, name: &str) -> (Handle<StandardMaterial>, StandardMaterial) {
    let e = named(app, name);
    let h = app.world().get::<MeshMaterial3d<StandardMaterial>>(e).expect("a standard material").0.clone();
    let m = app.world().resource::<Assets<StandardMaterial>>().get(&h).unwrap().clone();
    (h, m)
}

fn standard_game(cube: &str) -> String {
    format!("import {{ app }} from \"bevy-ts\";\n\napp.scene(\"s\", () => (\n  <entity name=\"cube\">\n    <mesh shape=\"cuboid\" />\n    {cube}\n  </entity>\n));\n")
}

#[test]
fn a_material_alpha_blends_by_default_and_an_explicit_mode_wins() {
    let dir = project("materials-alpha", &[("game.tsx", &standard_game("<material color=\"#ff0000\" alpha={0.5} />"))]);
    let mut app = material_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let (_, m) = standard(&mut app, "cube");
    assert_eq!((m.base_color.alpha(), m.alpha_mode), (0.5, AlphaMode::Blend));

    write(&dir, "game.tsx", &standard_game("<material color=\"#ff0000\" alpha={0.5} alphaMode={{ mask: 0.3 }} />"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    assert_eq!(standard(&mut app, "cube").1.alpha_mode, AlphaMode::Mask(0.3));

    // A colour with alpha keeps it, and blends by the same rule; opaque stays opaque.
    write(&dir, "game.tsx", &standard_game("<material color=\"#ff000080\" />"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    let (_, m) = standard(&mut app, "cube");
    assert!((m.base_color.alpha() - 128.0 / 255.0).abs() < 1e-6);
    assert_eq!(m.alpha_mode, AlphaMode::Blend);
    write(&dir, "game.tsx", &standard_game("<material color=\"#ff0000\" />"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    assert_eq!(standard(&mut app, "cube").1.alpha_mode, AlphaMode::Opaque);
}

#[test]
fn a_bad_alpha_or_mode_fails_at_the_element() {
    for (props, says) in [("alpha={2}", "alpha takes 0 to 1"), ("alphaMode=\"glass\"", "unknown alphaMode")] {
        let dir = project("materials-alpha-bad", &[("game.tsx", &standard_game(&format!("<material color=\"#ff0000\" {props} />")))]);
        let mut app = material_app(&dir.join("game.tsx"));
        let r = reload(&mut app, false);
        assert_eq!(r.status, ReloadStatus::Failed);
        let e = &r.errors[0];
        assert!(e.message.contains("<material>") && e.message.contains(says), "{e:?}");
        assert_eq!(e.location.clone().expect("the element").line, 6);
    }
}

const FADE: &str = r##"import { app, signal, Update, type Local, type u32 } from "bevy-ts";
import { MaterialAlpha } from "bevy-ts/bevy";

export const [k, setK] = signal(1);
export function fade(frames: Local<{ n: u32 }>) { frames.n += 1; setK(Math.max(0, 1 - frames.n / 10)); }
app.addSystems(Update, fade);

app.scene("s", () => (
  <entity name="root">
    <entity name="cube">
      <mesh shape="cuboid" />
      <material color="#ff0000" />
      <MaterialAlpha alpha={() => k()} />
    </entity>
    <entity name="twin">
      <mesh shape="cuboid" />
      <material color="#ff0000" />
    </entity>
  </entity>
));
"##;

#[test]
fn a_bound_material_alpha_fades_a_shape_frame_by_frame() {
    let dir = project("materials-fade", &[("game.tsx", FADE)]);
    let mut app = material_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    let (copy, _) = standard(&mut app, "cube");
    let (shared, twin) = standard(&mut app, "twin");
    assert_ne!(copy, shared, "the faded cube draws its own copy");
    let assets = app.world().resource::<Assets<StandardMaterial>>().len();
    let mut last = f32::MAX;
    for _ in 0..12 {
        app.update();
        let (h, m) = standard(&mut app, "cube");
        assert_eq!(h, copy, "edited in place");
        assert!(m.base_color.alpha() <= last, "{} after {last}", m.base_color.alpha());
        last = m.base_color.alpha();
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), assets, "no asset added");
    }
    assert_eq!(last, 0.0);
    assert_eq!(standard(&mut app, "cube").1.alpha_mode, AlphaMode::Blend);
    assert_eq!((twin.base_color.alpha(), twin.alpha_mode), (1.0, AlphaMode::Opaque));
    assert_eq!(standard(&mut app, "twin").0, shared, "the twin keeps the shared material");

    // A TS save that changes the cube's colour keeps it faded, now blue.
    let cube = named(&mut app, "cube");
    write(&dir, "game.tsx", &FADE.replacen("<material color=\"#ff0000\" />", "<material color=\"#0000ff\" />", 1));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 3);
    assert_eq!(named(&mut app, "cube"), cube);
    let (h, m) = standard(&mut app, "cube");
    let c = m.base_color.to_srgba();
    assert_eq!((c.red, c.blue), (0.0, 1.0));
    assert!(m.base_color.alpha() < 1.0 && m.alpha_mode == AlphaMode::Blend);
    assert_ne!(h, standard(&mut app, "twin").0);

    // A save without MaterialAlpha: the cube is opaque blue again.
    write(&dir, "game.tsx", &FADE.replacen("<material color=\"#ff0000\" />", "<material color=\"#0000ff\" />", 1).replace("<MaterialAlpha alpha={() => k()} />", ""));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    let m = standard(&mut app, "cube").1;
    assert_eq!((m.base_color.alpha(), m.alpha_mode), (1.0, AlphaMode::Opaque));
}
