//! Reflected values as a picture (reflect-enums-color): a TSX `<Projection>` that is orthographic
//! draws near and far cubes the same size, and a light's TSX `color` warms what it lights. Drawn
//! to an offscreen image, no window shown.
#![cfg(target_os = "macos")]

mod common;

use bevy::camera::RenderTarget;
use bevy::light::DirectionalLight;
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
    panic!("the picture never settled; last middle row: {:?}", last.map(|i| widths(&i)));
}

fn px(img: &Image, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * W + x) * 4) as usize;
    let d = img.data.as_ref().unwrap();
    [d[i], d[i + 1], d[i + 2]]
}

/// The widths of the lit runs left and right of the middle column, along the middle row.
fn widths(img: &Image) -> (u32, u32) {
    let lit = |x| px(img, x, H / 2).iter().any(|c| *c > 64);
    ((0..W / 2).filter(|x| lit(*x)).count() as u32, (W / 2..W).filter(|x| lit(*x)).count() as u32)
}

fn cubes(projection: &str) -> String {
    format!(
        r##"import {{ app }} from "bevy-ts";
import {{ Camera3d, Projection, Tonemapping, Transform }} from "bevy-ts/bevy";

app.scene("s", () => (
  <>
    <entity name="cam"><Camera3d /><Tonemapping value="None" /><Transform translation={{[0, 0, 10]}} />{projection}</entity>
    <entity name="near"><Transform translation={{[-3, 0, 4]}} /><mesh shape="cuboid" size={{[2, 2, 2]}} /><material color="#ffffff" unlit /></entity>
    <entity name="far"><Transform translation={{[3, 0, -20]}} /><mesh shape="cuboid" size={{[2, 2, 2]}} /><material color="#ffffff" unlit /></entity>
  </>
));
"##
    )
}

#[test]
fn an_orthographic_camera_draws_near_and_far_cubes_the_same_size() {
    let ortho = "<Projection value={{ Orthographic: { scaling_mode: { FixedVertical: { viewport_height: 12 } } } }} />";
    let dir = project("reflect-render-ortho", &[("game.tsx", &cubes(ortho))]);
    let mut app = app(&dir.join("game.tsx"));
    let img = picture(&mut app, |i| {
        let (l, r) = widths(i);
        l > 0 && r > 0
    });
    let (near, far) = widths(&img);
    assert!(near.abs_diff(far) <= 1, "orthographic: near {near} px, far {far} px");

    // The same scene in perspective: the far cube is smaller.
    write(&dir, "game.tsx", &cubes("<Projection value={{ Perspective: {} }} />"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    let img = picture(&mut app, |i| {
        let (l, r) = widths(i);
        l > 0 && r > 0 && l.abs_diff(r) > 1
    });
    let (near, far) = widths(&img);
    assert!(far < near, "perspective: near {near} px, far {far} px");
}

fn lit(color: &str) -> String {
    format!(
        r##"import {{ app, hex, srgb }} from "bevy-ts";
import {{ Camera3d, DirectionalLight, Tonemapping, Transform }} from "bevy-ts/bevy";

app.scene("s", () => (
  <>
    <entity name="cam"><Camera3d /><Tonemapping value="None" /><Transform translation={{[0, 0, 6]}} /></entity>
    <entity name="sun"><DirectionalLight illuminance={{2000}} color={{{color}}} /><Transform translation={{[0, 0, 1]}} /></entity>
    <entity name="cube"><Transform translation={{[0, 0, 0]}} /><mesh shape="cuboid" size={{[2, 2, 2]}} /><material color="#ffffff" /></entity>
  </>
));
"##
    )
}

#[test]
fn a_warm_light_colour_warms_the_lit_pixel() {
    let dir = project("reflect-render-light", &[("game.tsx", &lit("hex(\"#ffffff\")"))]);
    let mut app = app(&dir.join("game.tsx"));
    let white = picture(&mut app, |i| px(i, W / 2, H / 2)[0] > 64);
    let [r, _, b] = px(&white, W / 2, H / 2);
    assert!(r.abs_diff(b) <= 3, "a white light: red {r} and blue {b} equal");

    write(&dir, "game.tsx", &lit("srgb(1, 0.6, 0.3)"));
    assert_eq!(reload(&mut app, false).status, ReloadStatus::Applied);
    let world = app.world_mut();
    let l = *world.query::<&DirectionalLight>().single(world).unwrap();
    assert_eq!(l.color, Color::srgb(1.0, 0.6, 0.3), "the reload wrote the colour");
    let warm = picture(&mut app, |i| {
        let [r, _, b] = px(i, W / 2, H / 2);
        r > 64 && r > b.saturating_add(20)
    });
    let [r, _, b] = px(&warm, W / 2, H / 2);
    assert!(r > b, "a warm light: red {r} above blue {b}");
}
