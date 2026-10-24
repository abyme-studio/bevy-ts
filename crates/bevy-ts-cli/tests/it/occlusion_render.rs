//! `<OcclusionCulling />` alone renders (occlusion-culling-prereqs): bevy-ts makes `DepthPrepass` a
//! required component of it, so Bevy's depth pyramid has a depth texture it can bind. Drawn to an
//! offscreen image, no window shown. Bevy issue bevyengine/bevy#22747.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::camera::RenderTarget;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::render::occlusion_culling::OcclusionCulling;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts_cli::compile::Compiler;
use bevy_ts_v8::V8Options;
use common::{project, reload, write};
use std::path::Path;
use std::sync::{Arc, Mutex};

const W: u32 = 320;
const H: u32 = 180;

#[derive(Resource, Clone)]
struct Target(Handle<Image>);

fn app(entry: &Path) -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(V8Options::default())).dev(true))
    .add_plugins(bevy_ts_tsx::TsxPlugin)
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
    // Every camera the scene spawns draws to the image.
    app.add_observer(|add: On<Add<Camera>>, target: Res<Target>, mut commands: Commands| {
        commands.entity(add.entity).insert(RenderTarget::Image(target.0.clone().into()));
    });
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    app
}

/// Draws until `done` accepts a picture; panics with the last one's summary after many frames.
fn picture(app: &mut App, done: impl Fn(&Image) -> bool) -> Image {
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
            if done(&img) {
                return img;
            }
            last = Some(img);
        }
    }
    panic!("the picture never settled; last: {}", last.is_some());
}

fn px(img: &Image, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * W + x) * 4) as usize;
    let d = img.data.as_ref().unwrap();
    [d[i], d[i + 1], d[i + 2]]
}


fn lit(img: &Image) -> bool {
    px(img, W / 2, H / 2).iter().any(|c| *c > 64)
}

fn scene(camera: &str) -> String {
    format!(
        r##"import {{ app }} from "bevy-ts";
import {{ Camera3d, DepthPrepass, OcclusionCulling, Tonemapping, Transform }} from "bevy-ts/bevy";

app.scene("s", () => (
  <>
    <entity name="cam"><Camera3d /><Tonemapping value="None" /><Transform translation={{[0, 0, 6]}} />{camera}</entity>
    <entity name="cube"><Transform translation={{[0, 0, 0]}} /><mesh shape="cuboid" size={{[2, 2, 2]}} /><material color="#ffffff" unlit /></entity>
  </>
));
"##
    )
}

fn prepasses(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query_filtered::<(), (With<Camera3d>, With<DepthPrepass>)>().iter(world).count()
}

#[test]
fn occlusion_culling_alone_brings_a_depth_prepass_and_draws() {
    let dir = project("occlusion-render-alone", &[("game.tsx", &scene("<OcclusionCulling />"))]);
    let mut app = app(&dir.join("game.tsx"));
    // A missing prepass quits the app on the "downsample depth bind group" error: no picture.
    let img = picture(&mut app, lit);
    assert!(app.should_exit().is_none(), "the app quit");
    assert!(lit(&img), "the cube is drawn");
    assert_eq!(prepasses(&mut app), 1, "the camera got a DepthPrepass");
    let world = app.world_mut();
    let msaa = *world.query_filtered::<&Msaa, With<Camera3d>>().single(world).unwrap();
    assert_eq!(msaa, Msaa::Sample4, "MSAA stays on");
}

#[test]
fn a_written_depth_prepass_is_kept_without_occlusion_culling() {
    let both = scene("<OcclusionCulling /><DepthPrepass />");
    let dir = project("occlusion-render-written", &[("game.tsx", &both)]);
    let mut app = app(&dir.join("game.tsx"));
    picture(&mut app, lit);
    assert_eq!(prepasses(&mut app), 1, "one DepthPrepass");

    write(&dir, "game.tsx", &scene("<DepthPrepass />"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    picture(&mut app, lit);
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<(), With<OcclusionCulling>>().iter(world).count(), 0);
    assert_eq!(prepasses(&mut app), 1, "the written DepthPrepass stays");
}
