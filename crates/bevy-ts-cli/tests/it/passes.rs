//! Script passes from TSX (change `script-fullscreen-pass`): `pass()` and `<pass kind>` on a
//! `Camera2d` draw its image through the WGSL, a bound param writes the camera's bytes the frame it
//! changes, a `.wgsl` save redraws with no new generation, and errors point at the element or the
//! author's line. Drawn to an offscreen image, no window shown.
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
use bevy_ts_material::ScriptMaterialPlugin;
use bevy_ts_material::pass::slot_bytes;
use bevy_ts_v8::V8Options;
use common::{logs, project, reload, write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const W: u32 = 160;
const H: u32 = 90;

// Line 5 is the return.
const WGSL: &str = "import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;\n\n@fragment\nfn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {\n    return vec4<f32>(params.tint.rgb * params.amount, 1.0);\n}\n";

// `<pass>` is on line 14.
const GAME: &str = r##"import { app, Component, u8, signal, Update, type Query } from "bevy-ts";
import { Camera2d, Tonemapping } from "bevy-ts/bevy";
import { color, f32, pass } from "bevy-ts/material";
import dusk from "./dusk.wgsl";

export class Go extends Component { static path = "game::Go"; declare on: u8 }
export const Dusk = pass("game::Dusk", { params: { amount: f32, tint: color }, fragment: dusk });
export const [amount, setAmount] = signal(0.25);
export function step(q: Query<[Go]>) { q.each((g) => { if (g.on === 1) setAmount(1); }); }
app.addSystems(Update, step);

app.scene("s", () => (
  <entity name="cam"><Camera2d /><Tonemapping value="None" />
    <pass kind={Dusk} amount={() => amount()} tint="#ff0000" />
  </entity>
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
        commands.entity(add.entity).insert(RenderTarget::Image(target.0.clone().into()));
    });
    app
}

fn setup(name: &str) -> (common::TestDir, App) {
    let dir = project(name, &[("game.tsx", GAME), ("dusk.wgsl", WGSL)]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    (dir, app)
}

fn camera(app: &mut App) -> Entity {
    let world = app.world_mut();
    world.query_filtered::<(Entity, &Name), With<Camera2d>>().iter(world).find(|(_, n)| n.as_str() == "cam").map(|(e, _)| e).expect("the camera")
}

/// The camera's `amount`, from its slot component.
fn amount(app: &mut App) -> Option<f32> {
    let cam = camera(app);
    slot_bytes(app.world(), cam, 0).map(|b| f32::from_le_bytes(b[24..28].try_into().unwrap()))
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

/// Saves `dusk.wgsl` through the dev channel, as `bevy-ts dev`'s watcher does.
fn save_wgsl(app: &mut App, dir: &Path, src: &str) {
    let path = dir.join("dusk.wgsl").canonicalize().unwrap();
    std::fs::write(&path, src).unwrap();
    let (tx, channel) = DevChannel::new();
    app.insert_resource(channel);
    tx.send(DevEvent::FilesChanged(vec![path])).unwrap();
    app.update();
}

#[test]
fn a_tsx_pass_on_a_camera2d_draws_and_a_bound_amount_writes_that_frame() {
    let (_dir, mut app) = setup("passes-bound");
    // 0.25 red, linear, written to an sRGB target: about 137.
    middle(&mut app, "amount 0.25", |p| (130..=145).contains(&p[0]) && p[1] < 5 && p[2] < 5);
    assert_eq!(amount(&mut app), Some(0.25));
    let (id, _) = plugin_api::component_by_name(app.world(), "game::Go").unwrap();
    let e = app.world_mut().spawn_empty().id();
    // SAFETY: Go is one u8.
    unsafe { bevy_ts::commands::insert_bytes(&mut app.world_mut().entity_mut(e), id, &[1]) };
    // The system sets the signal in Update; the binding writes the camera's bytes that frame.
    app.update();
    assert_eq!(amount(&mut app), Some(1.0));
    middle(&mut app, "amount 1", |p| p[0] > 250 && p[1] < 5 && p[2] < 5);
}

#[test]
fn a_pass_off_a_camera2d_or_twice_fails_at_the_element() {
    let off = GAME.replace("<Camera2d /><Tonemapping value=\"None\" />", "<Transform />");
    let dir = project("passes-off-camera", &[("game.tsx", &off.replace("import { Camera2d, Tonemapping }", "import { Transform }")), ("dusk.wgsl", WGSL)]);
    let mut app = app(&dir.join("game.tsx"));
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = r.errors.iter().find(|e| e.message.contains("<pass")).unwrap_or_else(|| panic!("{:?}", r.errors));
    assert!(e.message.contains("Camera2d"), "{}", e.message);
    assert_eq!(e.location.as_ref().map(|l| l.line), Some(14), "{e:?}");
    let twice = GAME.replace("tint=\"#ff0000\" />", "tint=\"#ff0000\" />\n    <pass kind={Dusk} amount={0.5} />");
    write(&dir, "game.tsx", &twice);
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Failed);
    let e = r.errors.iter().find(|e| e.message.contains("twice")).unwrap_or_else(|| panic!("{:?}", r.errors));
    assert!(e.message.contains("game::Dusk"), "{}", e.message);
    assert_eq!(e.location.as_ref().map(|l| l.line), Some(15), "{e:?}");
    let unknown = GAME.replace("tint=\"#ff0000\"", "glow={2}");
    write(&dir, "game.tsx", &unknown);
    let r = reload(&mut app, false);
    let e = r.errors.iter().find(|e| e.message.contains("glow")).unwrap_or_else(|| panic!("{:?}", r.errors));
    assert!(e.message.contains("it has params amount, tint"), "{}", e.message);
}

#[test]
fn a_wgsl_save_redraws_with_no_new_generation() {
    let (dir, mut app) = setup("passes-save");
    middle(&mut app, "red", |p| (130..=145).contains(&p[0]) && p[2] < 5);
    let (generation, cam) = (app.world().resource::<Generations>().live, camera(&mut app));
    save_wgsl(&mut app, &dir, &WGSL.replace("params.tint.rgb * params.amount", "vec3<f32>(0.0, 0.0, 1.0) * params.amount"));
    middle(&mut app, "blue after the save", |p| p[0] < 5 && (130..=145).contains(&p[2]));
    assert_eq!(app.world().resource::<Generations>().live, generation, "no new generation");
    assert_eq!(camera(&mut app), cam, "no entity respawned");
}

#[test]
fn a_wgsl_error_names_the_pass_and_the_authors_line() {
    let (dir, mut app) = setup("passes-error");
    let before = middle(&mut app, "red", |p| (130..=145).contains(&p[0]) && p[2] < 5);
    save_wgsl(&mut app, &dir, &WGSL.replace("vec4<f32>(params.tint.rgb * params.amount, 1.0)", "1u"));
    let e = logs(&app).into_iter().rev().find(|e| e.level == LogLevel::Error).expect("an error");
    let at = e.location.clone().unwrap_or_else(|| panic!("no location: {}", e.message));
    assert!(e.message.contains("game::Dusk") && e.message.contains("previous shader keeps drawing"), "{}", e.message);
    assert!(at.file.ends_with("dusk.wgsl") && at.line == 5, "{at:?}: {}", e.message);
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(middle(&mut app, "the old shader", |_| true), before, "the previous pass keeps drawing");
}
