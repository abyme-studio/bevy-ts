//! Owned script materials as a picture (material-params): `assets.set` on a `Kind.add` instance
//! redraws the same asset in the new colour, and writing it every frame builds no new pipeline.
//! Drawn to an offscreen image, no window shown.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::{PipelineCache, TextureFormat};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::render::RenderApp;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts::reflect_asset::{ScriptAssets, flush, host_call};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_material::{ScriptMaterial, ScriptMaterial3d, ScriptMaterialPlugin};
use bevy_ts_v8::V8Options;
use common::{project, reload};
use serde_json::json;
use std::path::Path;
use std::sync::{Arc, Mutex};

const W: u32 = 160;
const H: u32 = 90;

const WGSL: &str = "import bevy_pbr::render::forward_io::VertexOutput;\n\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {\n    return params.color * params.pulse;\n}\n";

const GAME: &str = r##"import { app } from "bevy-ts";
import { Camera3d, Tonemapping, Transform } from "bevy-ts/bevy";
import { color, f32, material } from "bevy-ts/material";
import glow from "./glow.wgsl";

export const Glow = material("game::Glow", { params: { color: color, pulse: f32 }, fragment: glow });
export const g = Glow.add({ color: "#ff0000", pulse: 1 });

app.scene("s", () => (
  <>
    <entity name="cam"><Camera3d /><Tonemapping value="None" /><Transform translation={[0, 0, 6]} /></entity>
    <entity name="cube"><mesh shape="cuboid" size={[3, 3, 3]} /><material asset={g} /></entity>
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
            // The render world stays a sub-app, for its PipelineCache.
            .disable::<PipelinedRenderingPlugin>()
            .set(common::sync_pipelines())
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins((bevy_ts_tsx::TsxPlugin, ScriptMaterialPlugin))
    .insert_resource(ClearColor(Color::BLACK))
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
    let r = reload(&mut app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
    app
}

/// Draws until `done` accepts the middle pixel; panics with the last one after many frames.
fn middle(app: &mut App, done: impl Fn([u8; 3]) -> bool) -> [u8; 3] {
    let target = app.world().resource::<Target>().0.clone();
    let shot: Arc<Mutex<Option<Image>>> = Arc::default();
    let mut last = None;
    for frame in 0..900 {
        app.update();
        if frame % 10 == 5 {
            let s = shot.clone();
            app.world_mut().spawn(Screenshot::image(target.clone())).observe(move |e: On<ScreenshotCaptured>| {
                *s.lock().unwrap() = Some(e.image.clone());
            });
        }
        if let Some(img) = shot.lock().unwrap().take() {
            let i = ((H / 2 * W + W / 2) * 4) as usize;
            let d = img.data.as_ref().unwrap();
            let px = [d[i], d[i + 1], d[i + 2]];
            if done(px) {
                return px;
            }
            last = Some(px);
        }
    }
    panic!("the picture never settled; last middle pixel: {last:?}");
}

fn held(app: &mut App) -> AssetId<ScriptMaterial> {
    let world = app.world_mut();
    world.query::<&ScriptMaterial3d>().single(world).expect("one script-material entity").0.id()
}

/// `assets.set(g, patch)` as a script's call reaches the engine, then the end of its run.
fn set(app: &mut App, id: AssetId<ScriptMaterial>, patch: serde_json::Value) {
    let n = app.world().resource::<ScriptAssets>().ref_of(id.untyped()).expect("a script-made asset");
    let r = json!({ "$asset": n, "type": "bevy_ts_material::render::ScriptMaterial" });
    host_call(app.world(), "assets.set", &json!([r, patch]).to_string()).unwrap();
    flush(app.world_mut());
}

fn pipelines(app: &App) -> usize {
    app.sub_app(RenderApp).world().resource::<PipelineCache>().pipelines().count()
}

#[test]
fn a_set_redraws_the_same_asset_in_its_new_colour() {
    let dir = project("material-params-render-set", &[("game.tsx", GAME), ("glow.wgsl", WGSL)]);
    let mut app = app(&dir.join("game.tsx"));
    let red = middle(&mut app, |[r, g, b]| r > 200 && g < 30 && b < 30);
    let id = held(&mut app);
    set(&mut app, id, json!({ "color": "#0000ff" }));
    let blue = middle(&mut app, |[r, g, b]| b > 200 && r < 30 && g < 30);
    assert_eq!(held(&mut app), id, "the same AssetId draws: {red:?} then {blue:?}");
    assert_eq!(app.world().resource::<Assets<ScriptMaterial>>().len(), 1, "no new asset");
}

#[test]
fn writing_every_frame_builds_no_new_pipeline() {
    let dir = project("material-params-render-frames", &[("game.tsx", GAME), ("glow.wgsl", WGSL)]);
    let mut app = app(&dir.join("game.tsx"));
    middle(&mut app, |[r, ..]| r > 200);
    let id = held(&mut app);
    let before = pipelines(&app);
    for i in 0..60 {
        set(&mut app, id, json!({ "pulse": 0.5 + i as f64 / 120.0 }));
        app.update();
    }
    // The last write drawn: pulse 0.99 of red.
    middle(&mut app, |[r, ..]| r > 200);
    assert_eq!(pipelines(&app), before, "render pipelines before and after 60 writes");
    assert_eq!(held(&mut app), id);
    assert_eq!(app.world().resource::<Assets<ScriptMaterial>>().len(), 1, "no new asset");
}
