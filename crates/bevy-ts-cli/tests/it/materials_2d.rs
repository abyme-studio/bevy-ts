//! Script 2D materials from TSX (change `script-material2d`): `material2d()` and `<material2d kind>`
//! beside a `<mesh2d>` draw it through the WGSL, a bound instance prop writes the entity's row, a
//! `.wgsl` save redraws with no new generation, and errors point at the element or the author's
//! line. Drawn to an offscreen image, no window shown.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::generation::{DevChannel, DevEvent, DevSource, Generations, ReloadStatus};
use bevy_ts::log::LogLevel;
use bevy_ts::plugin_api;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_material::{MaterialKinds, MeshScriptMaterial2d, ScriptInstance, ScriptMaterial2d, ScriptMaterialPlugin};
use bevy_ts_v8::V8Options;
use common::{logs, project, reload, write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const W: u32 = 160;
const H: u32 = 90;

// Line 5 is the return.
const WGSL: &str = "import bevy_sprite_render::mesh2d::vertex_output::VertexOutput;\n\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {\n    return vec4<f32>(params.sea.rgb * instance(in.instance_index).land, 1.0);\n}\n";

// `<material2d>` is on line 16.
const GAME: &str = r##"import { app, Component, u8, signal, Update, type Query } from "bevy-ts";
import { Camera2d, Tonemapping } from "bevy-ts/bevy";
import { color, f32, material2d } from "bevy-ts/material";
import planet from "./planet.wgsl";

export class Go extends Component { static path = "game::Go"; declare on: u8 }
export const Planet = material2d("game::Planet", { params: { sea: color }, instance: { land: f32 }, fragment: planet });
export const [cutoff, setCutoff] = signal(0.25);
export function step(q: Query<[Go]>) { q.each((g) => { if (g.on === 1) setCutoff(1); }); }
app.addSystems(Update, step);

app.scene("s", () => (
  <>
    <entity name="cam"><Camera2d /><Tonemapping value="None" /></entity>
    <entity name="planet"><mesh2d shape="rectangle" size={[64, 64]} />
      <material2d kind={Planet} sea="#ff0000" land={() => cutoff()} />
    </entity>
  </>
));
"##;

#[derive(Resource, Clone)]
struct Target(Handle<Image>);

fn app(entry: &Path) -> App {
    let mut app = common::render_app();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<PipelinedRenderingPlugin>()
            .set(common::sync_pipelines())
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins((bevy_ts_tsx::TsxPlugin, ScriptMaterialPlugin))
    .insert_resource(DevSource(Arc::new(Compiler::new(entry, true).unwrap())));
    while app.plugins_state() == bevy::app::PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    let mut image = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
    image.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    app.insert_resource(Target(target));
    app.add_observer(|add: On<Add<Camera>>, target: Res<Target>, mut commands: Commands| {
        commands.entity(add.entity).insert((RenderTarget::Image(target.0.clone().into()), Msaa::Off));
    });
    app
}

fn setup(name: &str) -> (common::TestDir, App) {
    let dir = project(name, &[("game.tsx", GAME), ("planet.wgsl", WGSL)]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    (dir, app)
}

fn planet(app: &mut App) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &Name)>().iter(world).find(|(_, n)| n.as_str() == "planet").map(|(e, _)| e).expect("the planet")
}

/// The planet's `land`, from its row.
fn land(app: &mut App) -> Option<f32> {
    let e = planet(app);
    app.world().get::<ScriptInstance>(e).map(|s| f32::from_bits(s.data[0]))
}

/// Draws until `done` accepts the middle pixel, with a deadline that holds on a loaded machine.
fn middle(app: &mut App, what: &str, done: impl Fn([u8; 3]) -> bool) -> [u8; 3] {
    const BOUND: Duration = Duration::from_secs(300);
    let target = app.world().resource::<Target>().0.clone();
    let shot: Arc<Mutex<Option<Image>>> = Arc::default();
    let start = Instant::now();
    let mut pending = false;
    let mut last = None;
    loop {
        app.update();
        if !pending {
            let s = shot.clone();
            app.world_mut().spawn(Screenshot::image(target.clone())).observe(move |e: On<ScreenshotCaptured>| {
                *s.lock().unwrap() = Some(e.image.clone());
            });
            pending = true;
        }
        if let Some(img) = shot.lock().unwrap().take() {
            pending = false;
            let d = img.data.as_ref().unwrap();
            let i = ((H / 2 * W + W / 2) * 4) as usize;
            let px = [d[i], d[i + 1], d[i + 2]];
            if done(px) {
                return px;
            }
            last = Some(px);
        }
        assert!(start.elapsed() < BOUND, "{what}: never drawn within {BOUND:?}; last middle pixel {last:?}");
    }
}

/// Saves `planet.wgsl` through the dev channel, as `bevy-ts dev`'s watcher does.
fn save_wgsl(app: &mut App, dir: &Path, src: &str) {
    let path = dir.join("planet.wgsl").canonicalize().unwrap();
    std::fs::write(&path, src).unwrap();
    let (tx, channel) = DevChannel::new();
    app.insert_resource(channel);
    tx.send(DevEvent::FilesChanged(vec![path])).unwrap();
    app.update();
}

#[test]
fn a_tsx_material2d_kind_draws_and_a_bound_instance_prop_writes_its_row() {
    let (_dir, mut app) = setup("materials-2d-bound");
    // 0.25 red, linear, written to an sRGB target: about 137.
    middle(&mut app, "land 0.25", |p| (130..=145).contains(&p[0]) && p[1] < 5 && p[2] < 5);
    assert_eq!(land(&mut app), Some(0.25));
    let e = planet(&mut app);
    let asset = app.world().get::<MeshScriptMaterial2d>(e).expect("a script 2D material").0.id();
    assert!(app.world().get::<MeshMaterial2d<ColorMaterial>>(e).is_none(), "no ColorMaterial");
    let (id, _) = plugin_api::component_by_name(app.world(), "game::Go").unwrap();
    let go = app.world_mut().spawn_empty().id();
    // SAFETY: Go is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(go), id, &[1]) };
    // The system sets the signal in Update; the binding writes the entity's row that frame.
    app.update();
    assert_eq!(land(&mut app), Some(1.0));
    assert_eq!(app.world().get::<MeshScriptMaterial2d>(e).unwrap().0.id(), asset, "the asset is unchanged");
    assert_eq!(app.world().resource::<Assets<ScriptMaterial2d>>().len(), 1);
    middle(&mut app, "land 1", |p| p[0] > 250 && p[1] < 5 && p[2] < 5);
}

#[test]
fn material2d_kind_refuses_a_sprite_a_3d_kind_and_a_bound_param_at_the_element() {
    let dir = project("materials-2d-refusals", &[("game.tsx", GAME), ("planet.wgsl", WGSL)]);
    let mut app = app(&dir.join("game.tsx"));
    let fails = |app: &mut App, src: &str, what: &str, line: u32| {
        write(&dir, "game.tsx", src);
        let r = reload(app, false);
        assert_eq!(r.status, ReloadStatus::Failed, "{what}");
        let e = r.errors.iter().find(|e| e.message.contains(what)).unwrap_or_else(|| panic!("no {what:?} in {:?}", r.errors));
        assert_eq!(e.location.as_ref().map(|l| l.line), Some(line), "{e:?}");
    };
    // On a sprite: the rectangle and a texture param instead.
    let sprite = GAME.replace("<mesh2d shape=\"rectangle\" size={[64, 64]} />", "<Sprite />").replace("import { Camera2d, Tonemapping }", "import { Camera2d, Sprite, Tonemapping }");
    fails(&mut app, &sprite, "<mesh2d shape=\"rectangle\"> with the image as a texture param", 16);
    // A 3D kind on <material2d>, and the 2D kind on <material>.
    let three = GAME.replace("import { color, f32, material2d }", "import { color, f32, material, material2d }").replace("material2d(\"game::Planet\"", "material(\"game::Planet\"");
    fails(&mut app, &three, "game::Planet is a 3D material", 16);
    let on_3d = GAME.replace("<mesh2d shape=\"rectangle\" size={[64, 64]} />", "<mesh shape=\"cuboid\" />").replace("<material2d kind", "<material kind");
    fails(&mut app, &on_3d, "game::Planet is a 2D material", 16);
    // A bound param: only instance params follow a binding.
    let bound = GAME.replace("sea=\"#ff0000\"", "sea={() => \"#ff0000\"}");
    fails(&mut app, &bound, "<material2d> props cannot be bound yet", 16);
    // A ColorMaterial prop the kind lacks.
    let color = GAME.replace("sea=\"#ff0000\"", "sea=\"#ff0000\" color=\"#fff\"");
    fails(&mut app, &color, "game::Planet has no param or texture color", 16);
    // One name, two spaces: the second call fails, naming the first.
    let twice = GAME.replace(
        "export const [cutoff",
        "export const Glow = material(\"game::Planet\", { params: { sea: color }, fragment: planet });\nexport const [cutoff",
    );
    let twice = twice.replace("import { color, f32, material2d }", "import { color, f32, material, material2d }");
    fails(&mut app, &twice, "is declared twice; material2d game::Planet at", 8);
}

#[test]
fn a_wgsl_save_redraws_with_no_new_generation() {
    let (dir, mut app) = setup("materials-2d-save");
    middle(&mut app, "red", |p| (130..=145).contains(&p[0]) && p[2] < 5);
    let (generation, e) = (app.world().resource::<Generations>().live, planet(&mut app));
    let built = app.world().resource::<MaterialKinds>().shaders_built;
    save_wgsl(&mut app, &dir, &WGSL.replace("params.sea.rgb", "vec3<f32>(0.0, 0.0, 1.0)"));
    middle(&mut app, "blue after the save", |p| p[0] < 5 && (130..=145).contains(&p[2]));
    assert_eq!(app.world().resource::<Generations>().live, generation, "no new generation");
    assert_eq!(planet(&mut app), e, "no entity respawned");
    assert_eq!(app.world().resource::<MaterialKinds>().shaders_built, built + 1, "replaced in place");
}

#[test]
fn a_wgsl_error_names_the_kind_and_the_authors_line() {
    let (dir, mut app) = setup("materials-2d-error");
    let before = middle(&mut app, "red", |p| (130..=145).contains(&p[0]) && p[2] < 5);
    save_wgsl(&mut app, &dir, &WGSL.replace("vec4<f32>(params.sea.rgb * instance(in.instance_index).land, 1.0)", "1u"));
    let e = logs(&app).into_iter().rev().find(|e| e.level == LogLevel::Error).expect("an error");
    let at = e.location.clone().unwrap_or_else(|| panic!("no location: {}", e.message));
    assert!(e.message.contains("game::Planet") && e.message.contains("previous shader keeps drawing"), "{}", e.message);
    assert!(at.file.ends_with("planet.wgsl") && at.line == 5, "{at:?}: {}", e.message);
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(middle(&mut app, "the old shader", |_| true), before, "the previous planet keeps drawing");
}
