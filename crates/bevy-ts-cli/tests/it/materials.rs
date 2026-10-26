//! Script materials end to end: a `.wgsl` import, `material()` in V8, `<material kind>` in TSX,
//! and the Rust kinds, across reloads and shader saves. No GPU: shader and material assets only,
//! but for `standard`, which draws standard materials headless (macOS, Metal).

use crate::common;

use bevy::prelude::*;
use bevy_ts::generation::{DevChannel, DevEvent, ReloadStatus};
use bevy_ts_material::{FadeMaterial, MaterialKinds, ScriptMaterial, ScriptMaterial3d, ScriptMaterialPlugin};
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

/// A `Glow` per row of `rows`, keyed by its pulse, so a new pulse is a new key; `tick` sets `rows`
/// each frame.
fn rows(tick: &str) -> String {
    format!(
        "import {{ app, signal, For, Update, type Local, type u32 }} from \"bevy-ts\";\nimport {{ f32, material }} from \"bevy-ts/material\";\nimport glow from \"./glow.wgsl\";\n\nexport const Glow = material(\"game::Glow\", {{ params: {{ pulse: f32 }}, fragment: glow }});\nexport const [rows, setRows] = signal<{{ name: string; pulse: number }}[]>([{{ name: \"a\", pulse: 1 }}, {{ name: \"b\", pulse: 1 }}]);\nexport function tick(frames: Local<{{ n: u32 }}>) {{ frames.n += 1; {tick} }}\napp.addSystems(Update, tick);\n\napp.scene(\"s\", () => (\n  <For each={{rows}} key={{(r: {{ name: string; pulse: number }}) => `${{r.name}}-${{r.pulse}}`}}>\n    {{(r: {{ name: string; pulse: number }}) => <entity name={{r.name}}><mesh shape=\"cuboid\" /><material kind={{Glow}} pulse={{r.pulse}} /></entity>}}\n  </For>\n));\n"
    )
}

fn kept(app: &App) -> (usize, usize) {
    (app.world().resource::<MaterialKinds>().live["game::Glow"].instances.len(), app.world().resource::<Assets<ScriptMaterial>>().len())
}

#[test]
fn props_that_change_every_frame_free_the_old_instances() {
    let (_dir, mut app) = setup("materials-sweep", &rows("setRows([{ name: \"a\", pulse: frames.n }]);"));
    updates(&mut app, 2);
    // The id, not the handle: a handle held here would keep the instance.
    let first = cube(&mut app).1.id();
    for i in 0..120 {
        app.update();
        let (instances, assets) = kept(&app);
        assert!(instances <= 2 && assets <= 2, "frame {i}: {instances} instances, {assets} assets");
    }
    assert_ne!(cube(&mut app).1.id(), first, "the pulse moved on");
}

#[test]
fn one_of_two_sharers_removed_keeps_the_others_instance() {
    let (_dir, mut app) = setup("materials-sharers", &rows("if (frames.n === 3) setRows([{ name: \"b\", pulse: 1 }]);"));
    app.update();
    let (a, b) = (named(&mut app, "a"), named(&mut app, "b"));
    let held = |app: &mut App, e: Entity| app.world().get::<ScriptMaterial3d>(e).unwrap().0.clone();
    let shared = held(&mut app, b);
    assert_eq!(held(&mut app, a), shared, "equal props, one instance");
    updates(&mut app, 4);
    assert!(app.world().get_entity(a).is_err(), "a is gone");
    assert_eq!(held(&mut app, b), shared, "b keeps the handle");
    assert_eq!(kept(&app), (1, 1));
    assert!(app.world().resource::<Assets<ScriptMaterial>>().get(&shared).is_some());
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

/// The fade copy a faded entity draws, which holds no `StandardMaterial` while faded.
fn faded(app: &mut App, name: &str) -> (Handle<FadeMaterial>, FadeMaterial) {
    let e = named(app, name);
    assert!(app.world().get::<MeshMaterial3d<StandardMaterial>>(e).is_none(), "{name} holds no StandardMaterial while faded");
    let h = app.world().get::<MeshMaterial3d<FadeMaterial>>(e).expect("a fade copy").0.clone();
    let m = app.world().resource::<Assets<FadeMaterial>>().get(&h).unwrap().clone();
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
    let (copy, _) = faded(&mut app, "cube");
    let (shared, twin) = standard(&mut app, "twin");
    let count = |app: &App| app.world().resource::<Assets<StandardMaterial>>().len() + app.world().resource::<Assets<FadeMaterial>>().len();
    let assets = count(&app);
    let mut last = f32::MAX;
    for _ in 0..12 {
        app.update();
        let (h, m) = faded(&mut app, "cube");
        assert_eq!(h, copy, "edited in place");
        assert!(m.extension.alpha <= last, "{} after {last}", m.extension.alpha);
        last = m.extension.alpha;
        assert_eq!(count(&app), assets, "no asset added");
    }
    assert_eq!(last, 0.0);
    assert_eq!(faded(&mut app, "cube").1.base.alpha_mode, AlphaMode::Opaque, "the copy keeps the original's mode");
    assert_eq!((twin.base_color.alpha(), twin.alpha_mode), (1.0, AlphaMode::Opaque));
    assert_eq!(standard(&mut app, "twin").0, shared, "the twin keeps the shared material");

    // A TS save that changes the cube's colour keeps it faded, now blue.
    let cube = named(&mut app, "cube");
    write(&dir, "game.tsx", &FADE.replacen("<material color=\"#ff0000\" />", "<material color=\"#0000ff\" />", 1));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 3);
    assert_eq!(named(&mut app, "cube"), cube);
    let m = faded(&mut app, "cube").1;
    let c = m.base.base_color.to_srgba();
    assert_eq!((c.red, c.blue), (0.0, 1.0));
    assert!(m.extension.alpha < 1.0 && m.base.alpha_mode == AlphaMode::Opaque);

    // A save without MaterialAlpha: the cube is opaque blue again.
    write(&dir, "game.tsx", &FADE.replacen("<material color=\"#ff0000\" />", "<material color=\"#0000ff\" />", 1).replace("<MaterialAlpha alpha={() => k()} />", ""));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    updates(&mut app, 2);
    let m = standard(&mut app, "cube").1;
    assert_eq!((m.base_color.alpha(), m.alpha_mode), (1.0, AlphaMode::Opaque));
}

/// `<material>` without `kind`, drawn: textures, two-sided planes and normal maps.
#[cfg(target_os = "macos")]
mod drawn {
    use super::common::*;
    use bevy::prelude::*;
    use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
    use bevy_ts::BevyTsPlugin;
    use bevy_ts::generation::{DevSource, ReloadStatus};
    use bevy_ts_cli::compile::Compiler;
    use bevy_ts_cli::headless::{self, Offscreen, SIZE};
    use std::sync::{Arc, Mutex};

    /// A PNG of `w` x `h` RGBA pixels, one colour.
    fn png(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba.repeat((w * h) as usize)).unwrap();
        out
    }

    /// The scene `s`, with a black background and the camera and sun `stage` spawns.
    fn draw(name: &str, stage: &str, scene: &str, textures: &[(&str, [u8; 4])]) -> Image {
        let textures: Vec<(&str, Vec<u8>)> = textures.iter().map(|(p, rgba)| (*p, png(4, 4, *rgba))).collect();
        draw_with(name, "", stage, scene, &[], &textures, Color::BLACK)
    }

    /// A 4 x 4 PNG, its left half `left` and its right half `right`.
    fn halves(left: [u8; 4], right: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 4, 4);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let row = [left, left, right, right].concat();
        enc.write_header().unwrap().write_image_data(&row.repeat(4)).unwrap();
        out
    }

    /// As `draw`, with `header` after the imports, `files` beside the game, and a `clear` colour.
    fn draw_with(name: &str, header: &str, stage: &str, scene: &str, files: &[(&str, &str)], textures: &[(&str, Vec<u8>)], clear: Color) -> Image {
        let game = format!(
            "import {{ app, Startup }} from \"bevy-ts\";\nimport type {{ Commands }} from \"bevy-ts\";\nimport {{ Camera3d, DirectionalLight, Transform }} from \"bevy-ts/bevy\";\n{header}\n\nexport function stage(commands: Commands) {{\n{stage}\n}}\napp.addSystems(Startup, stage);\n\napp.scene(\"s\", () => (\n  <>\n{scene}\n  </>\n));\n"
        );
        let mut all = vec![("game.tsx", game.as_str()), ("package.json", "{}")];
        all.extend_from_slice(files);
        let dir = project(name, &all);
        for (path, bytes) in textures {
            let p = dir.join("assets").join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }
        let mut app = App::new();
        app.add_plugins(headless::default_plugins(true).set(sync_pipelines()).set(headless::asset_plugin_unwatched(&dir.join("assets"))))
            .add_plugins(headless::HeadlessPlugin)
            .add_plugins(BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
            .add_plugins(bevy_ts_tsx::TsxPlugin)
            .add_plugins(bevy_ts_material::ScriptMaterialPlugin)
            .insert_resource(ClearColor(clear));
        app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.tsx"), true).unwrap())));
        app.finish();
        app.cleanup();
        let r = reload(&mut app, false);
        assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
        // Long enough for textures to load and pipelines to compile.
        for _ in 0..120 {
            app.update();
        }
        let shot: Arc<Mutex<Option<Image>>> = Arc::default();
        let into = shot.clone();
        let target = app.world().resource::<Offscreen>().0.clone();
        app.world_mut().spawn(Screenshot::image(target)).observe(move |c: On<ScreenshotCaptured>| {
            *into.lock().unwrap() = Some(c.image.clone());
        });
        for _ in 0..60 {
            app.update();
            if let Some(image) = shot.lock().unwrap().take() {
                let errors: Vec<_> = logs(&app).into_iter().filter(|e| e.level == bevy_ts::log::LogLevel::Error).map(|e| e.message).collect();
                assert!(errors.is_empty(), "{errors:?}");
                return image;
            }
        }
        panic!("no screenshot");
    }

    /// The colour at a fraction of the view's width and height.
    fn at(image: &Image, x: f32, y: f32) -> LinearRgba {
        let (px, py) = ((SIZE.0 as f32 * x) as u32, (SIZE.1 as f32 * y) as u32);
        image.get_color_at(px, py).unwrap().to_linear()
    }

    const CAMERA: &str = "  commands.spawn(Camera3d, Transform.lookingAt([0, 0, 3], [0, 0, 0]));";

    #[test]
    fn a_textured_cube_draws_the_image() {
        let scene = "    <entity>\n      <mesh shape=\"cuboid\" />\n      <material unlit baseColorTexture=\"textures/green.png\" />\n    </entity>";
        let image = draw("standard-textured", CAMERA, scene, &[("textures/green.png", [0, 255, 0, 255])]);
        let c = at(&image, 0.5, 0.5);
        // Tonemapped, so not full green, but green and nothing else (an untextured face is white).
        assert!(c.green > 0.3 && c.red < 0.1 && c.blue < 0.1, "the cube's face is the texture's green: {c:?}");
    }

    #[test]
    fn a_two_sided_plane_draws_its_back() {
        // Two planes facing up, seen from below and lit from below: only the two-sided one shows.
        let stage = "  commands.spawn(Camera3d, Transform.lookingAt([0, -3, 0.5], [0, 0, 0]));\n  commands.spawn(DirectionalLight.of({ shadow_maps_enabled: false }), Transform.lookingAt([0, -5, 0], [0, 0, 0]));";
        let scene = "    <entity>\n      <Transform translation={[-0.75, 0, 0]} />\n      <mesh shape=\"plane\" />\n      <material color=\"#ffffff\" />\n    </entity>\n    <entity>\n      <Transform translation={[0.75, 0, 0]} />\n      <mesh shape=\"plane\" />\n      <material color=\"#ffffff\" doubleSided cullMode={null} />\n    </entity>";
        let image = draw("standard-two-sided", stage, scene, &[]);
        let (one, two) = (at(&image, 0.3, 0.5), at(&image, 0.7, 0.5));
        assert!(one.red + one.green + one.blue < 0.01, "a one-sided plane's back is culled: {one:?}");
        assert!(two.red > 0.2, "a two-sided plane's back is drawn lit: {two:?}");
    }

    #[test]
    fn a_normal_map_shades_a_cube() {
        // Two white cubes in one light; the right one's normal map tilts every normal.
        let stage = format!("{CAMERA}\n  commands.spawn(DirectionalLight.of({{ shadow_maps_enabled: false }}), Transform.lookingAt([2, 3, 4], [0, 0, 0]));");
        let cube = |x: f32, extra: &str| format!("    <entity>\n      <Transform translation={{[{x}, 0, 0]}} />\n      <mesh shape=\"cuboid\" size={{[0.8, 0.8, 0.8]}} />\n      <material color=\"#ffffff\" {extra}/>\n    </entity>");
        let scene = format!("{}\n{}", cube(-0.6, ""), cube(0.6, "normalMapTexture=\"textures/tilt.png\" "));
        let image = draw("standard-normal-map", &stage, &scene, &[("textures/tilt.png", [240, 128, 160, 255])]);
        let (plain, mapped) = (at(&image, 0.38, 0.5), at(&image, 0.62, 0.5));
        let diff = (plain.red - mapped.red).abs() + (plain.green - mapped.green).abs() + (plain.blue - mapped.blue).abs();
        assert!(diff > 0.05, "the normal map changes the lit face: {plain:?} vs {mapped:?}");
    }

    /// Agent town's atlas: a pixel-art texture tiled four times across a face (`uvTransform` scale 4).
    /// Middle of the face, left to right: whether each sample is the atlas's green half.
    fn atlas_row(name: &str, settings: &str) -> Vec<bool> {
        let header = format!("import {{ Image }} from \"bevy-ts/bevy\";\nconst atlas = Image.load(\"textures/atlas.png\"{settings});");
        let scene = "    <entity>\n      <mesh shape=\"cuboid\" />\n      <material unlit baseColorTexture={atlas} uvTransform={{ scale: [4, 4] }} />\n    </entity>";
        let atlas = halves([0, 255, 0, 255], [255, 0, 0, 255]);
        let image = draw_with(name, &header, CAMERA, scene, &[], &[("textures/atlas.png", atlas)], Color::BLACK);
        (0..41).map(|i| at(&image, 0.42 + 0.004 * i as f32, 0.5)).map(|c| c.green > c.red).collect()
    }

    fn changes(row: &[bool]) -> usize {
        row.windows(2).filter(|w| w[0] != w[1]).count()
    }

    #[test]
    fn an_atlas_scaled_by_four_repeats_with_a_repeating_sampler() {
        let repeat = ", { sampler: { Descriptor: { address_mode_u: \"Repeat\", address_mode_v: \"Repeat\", mag_filter: \"Nearest\" } } }";
        let row = atlas_row("standard-atlas-repeat", repeat);
        assert!(changes(&row) >= 3, "green and red repeat across the face: {row:?}");
        // Bevy's default sampler clamps: past the first tile the face is the atlas's right edge.
        let row = atlas_row("standard-atlas-clamp", "");
        assert_eq!(changes(&row), 0, "clamped to the red edge: {row:?}");
        assert!(!row[0], "red: {row:?}");
    }

    /// A cube that adds over a red background: what is behind shows through its green.
    fn adds(c: LinearRgba) -> bool {
        c.red > 0.1 && c.green > 0.1
    }

    const RED: Color = Color::srgb(0.6, 0.0, 0.0);
    const GREEN: &str = "import bevy_pbr::render::forward_io::VertexOutput;\n\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {\n    return vec4<f32>(0.0, 0.6, 0.0, 1.0);\n}\n";

    #[test]
    fn a_script_material_with_alpha_mode_add_adds() {
        let header = "import { f32, material } from \"bevy-ts/material\";\nimport green from \"./green.wgsl\";\nexport const Add = material(\"game::Add\", { params: { k: f32 }, fragment: green, alphaMode: \"add\" });";
        let scene = "    <entity>\n      <mesh shape=\"cuboid\" />\n      <material kind={Add} k={1} />\n    </entity>";
        let image = draw_with("standard-add-script", header, CAMERA, scene, &[("green.wgsl", GREEN)], &[], RED);
        let c = at(&image, 0.5, 0.5);
        assert!(adds(c), "the cube adds its green to the red behind: {c:?}");
    }

    #[test]
    fn a_standard_material_with_alpha_mode_add_adds() {
        let scene = "    <entity>\n      <mesh shape=\"cuboid\" />\n      <material unlit color=\"#009900\" alphaMode=\"add\" />\n    </entity>";
        let image = draw_with("standard-add", "", CAMERA, scene, &[], &[], RED);
        let c = at(&image, 0.5, 0.5);
        assert!(adds(c), "the cube adds its green to the red behind: {c:?}");
    }
}

// Change material-params, stage 2: owned instances from `Kind.add`, written in place.

/// A `Glow` added at load, `<material asset={g} />` on a cube; `tick` runs each frame.
fn owned(params: &str, tick: &str) -> String {
    format!(
        "import {{ app, assets, Update, type Local, type u32 }} from \"bevy-ts\";\nimport {{ color, f32, material }} from \"bevy-ts/material\";\nimport glow from \"./glow.wgsl\";\n\nexport const Glow = material(\"game::Glow\", {{ params: {{ {params} }}, fragment: glow }});\nexport const g = Glow.add({{ color: \"#000000\", pulse: 2 }});\nexport function tick(frames: Local<{{ n: u32 }}>) {{ frames.n += 1; {tick} }}\napp.addSystems(Update, tick);\n\napp.scene(\"s\", () => (\n  <entity name=\"cube\">\n    <mesh shape=\"cuboid\" />\n    <material asset={{g}} />\n  </entity>\n));\n"
    )
}

/// `assets.get` of the script-made asset `h`, as a script's call reaches the engine.
fn get(app: &App, h: &Handle<ScriptMaterial>) -> serde_json::Value {
    let n = app.world().resource::<bevy_ts::reflect_asset::ScriptAssets>().ref_of(h.id().untyped()).expect("a script-made asset");
    let r = serde_json::json!([{ "$asset": n, "type": "bevy_ts_material::render::ScriptMaterial" }]);
    serde_json::from_str(&bevy_ts::reflect_asset::host_call(app.world(), "assets.get", &r.to_string()).unwrap()).unwrap()
}

fn draws_live_kind(app: &App, h: &Handle<ScriptMaterial>) -> bool {
    let m = app.world().resource::<Assets<ScriptMaterial>>().get(h).unwrap();
    std::sync::Arc::ptr_eq(&m.kind, &app.world().resource::<MaterialKinds>().live["game::Glow"].kind)
}

#[test]
fn an_owned_instance_is_written_in_place_and_read_back() {
    let (_dir, mut app) = setup("materials-owned-set", &owned("color: color, pulse: f32", "if (frames.n === 3) assets.set(g, { pulse: 3 });"));
    let (e, h) = cube(&mut app);
    assert!(draws_live_kind(&app, &h), "added while the script loaded, drawn by the kind once it applied");
    assert_eq!(f32_at(&app, &h, 16), 2.0);
    updates(&mut app, 5);
    assert_eq!(cube(&mut app), (e, h.clone()), "the same entity holds the same asset");
    assert_eq!(f32_at(&app, &h, 16), 3.0, "pulse written in place");
    assert_eq!(app.world().resource::<Assets<ScriptMaterial>>().len(), 1, "no new asset");
    assert!(app.world().resource::<MaterialKinds>().live["game::Glow"].instances.is_empty(), "never in the keyed cache");
    let read = get(&app, &h);
    assert_eq!(read["kind"], "game::Glow");
    assert_eq!(read["pulse"], 3.0);
    assert_eq!(read["color"]["LinearRgba"]["alpha"], 1.0);
}

/// Every `Modified` of a `ScriptMaterial`, each read once: the current buffer alone can hold a
/// frame's messages over several updates, as messages swap only after a fixed tick.
#[derive(Resource, Default)]
struct Modifieds(Vec<AssetId<ScriptMaterial>>);

fn count_modified(mut events: bevy::ecs::message::MessageReader<AssetEvent<ScriptMaterial>>, mut seen: ResMut<Modifieds>) {
    seen.0.extend(events.read().filter_map(|ev| if let AssetEvent::Modified { id } = ev { Some(*id) } else { None }));
}

#[test]
fn an_equal_set_sends_no_modified() {
    let (_dir, mut app) = setup("materials-owned-equal", &owned("color: color, pulse: f32", "assets.set(g, { pulse: 2, color: \"#000000\" });"));
    app.init_resource::<Modifieds>().add_systems(Last, count_modified);
    let (_, h) = cube(&mut app);
    // The asset's own Added and Modified, from the load and its kind applying.
    updates(&mut app, 2);
    let before = app.world().resource::<Modifieds>().0.iter().filter(|id| **id == h.id()).count();
    updates(&mut app, 5);
    let after = app.world().resource::<Modifieds>().0.iter().filter(|id| **id == h.id()).count();
    assert_eq!(after, before, "an equal set every frame marks nothing changed");
}

#[test]
fn a_set_naming_what_the_kind_lacks_fails_with_its_names() {
    let (_dir, mut app) = setup("materials-owned-unknown", &owned("color: color, pulse: f32", "if (frames.n === 2) assets.set(g, { speed: 1 });"));
    updates(&mut app, 4);
    let m = messages(&app);
    assert!(m.iter().any(|m| m.contains("game::Glow has no param or texture speed; it has color, pulse")), "{m:?}");
}

#[test]
fn a_ref_of_another_type_fails_at_the_element() {
    let src = owned("color: color, pulse: f32", "").replace("asset={g}", "asset={{ $asset: 1, type: \"bevy_pbr::pbr_material::StandardMaterial\" } as any}");
    let dir = project("materials-owned-type", &[("game.tsx", &src), ("glow.wgsl", WGSL)]);
    let mut app = material_app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = &r.errors[0];
    assert!(e.message.contains("<material asset>") && e.message.contains("not a bevy_ts_material::render::ScriptMaterial"), "{e:?}");
    assert_eq!(e.location.clone().expect("the element").line, 13);
    let src = owned("color: color, pulse: f32", "").replace("asset={g}", "asset={g} pulse={1}");
    write(&dir, "game.tsx", &src);
    let r = reload(&mut app, false);
    assert!(r.errors.iter().any(|e| e.message.contains("takes only asset and gltfMaterial, not pulse")), "{:?}", r.errors);
}

#[test]
fn a_stale_ref_fails_at_the_element() {
    // Ref 1 is another instance, so the element's prop changes from ref 2 to ref 1.
    let first = owned("color: color, pulse: f32", "").replace("export const g =", "export const other = Glow.add({});\nexport const g =");
    let (dir, mut app) = setup("materials-owned-stale", &first);
    // The first generation's ref 1, given after a reload made it stale.
    let src = owned("color: color, pulse: f32", "").replace("asset={g}", "asset={{ $asset: 1, type: \"bevy_ts_material::render::ScriptMaterial\" } as any}");
    write(&dir, "game.tsx", &src);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app.update();
    let m = messages(&app);
    assert!(m.iter().any(|m| m.contains("<material asset>: stale asset ref 1")), "{m:?}");
}

#[test]
fn a_schema_change_re_encodes_an_owned_instance() {
    let (dir, mut app) = setup("materials-owned-schema", &owned("color: color, pulse: f32", ""));
    let (_, old) = cube(&mut app);
    // `speed` before `pulse`: pulse moves from 16 to 20.
    write(&dir, "game.tsx", &owned("color: color, speed: f32, pulse: f32", ""));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    let (_, new) = cube(&mut app);
    assert_ne!(new, old, "the reload's own Kind.add");
    for h in [&old, &new] {
        assert!(draws_live_kind(&app, h));
        assert_eq!(f32_at(&app, h, 16), 0.0, "speed defaults to 0");
        assert_eq!(f32_at(&app, h, 20), 2.0, "pulse at its new offset");
    }
}
