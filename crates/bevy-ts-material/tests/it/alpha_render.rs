//! `MaterialAlpha` drawn (change `material-alpha`, task 3.1), into an offscreen image over a white
//! backdrop, left to right: a red cube built as `<material color="#ff0000" alpha={0.5} unlit>`
//! builds it, a red cube with `MaterialAlpha(0.5)`, its twin sharing the material with none, then
//! `ship.gltf` under a root with `MaterialAlpha(0.5)` and a second ship with none.

use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_material::{MaterialAlpha, ScriptMaterialRenderPlugin};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const W: u32 = 320;
const H: u32 = 180;
/// Where each thing stands, in x; the camera sees about 14.7 wide at z 0.
const XS: [f32; 5] = [-5.0, -2.5, 0.0, 2.5, 5.0];

#[test]
fn faded_shapes_and_scenes_draw_blended_and_their_twins_do_not() {
    let mut app = App::new();
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets");
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: assets.to_string_lossy().into_owned(), watch_for_changes_override: Some(false), ..default() }),
    )
    .add_plugins(ScriptMaterialRenderPlugin);
    while app.plugins_state() == bevy::app::PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    let world = app.world_mut();

    let mut target = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    world.spawn((
        Camera3d::default(),
        Camera { clear_color: ClearColorConfig::Custom(Color::WHITE), ..default() },
        Tonemapping::None,
        Msaa::Off,
        RenderTarget::Image(target.clone().into()),
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let cube = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(1.5, 1.5, 1.5));
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let half = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.0, 0.0, 0.5), alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
    let red = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.0, 0.0), unlit: true, ..default() });
    let at = |i: usize| Transform::from_xyz(XS[i], 0.0, 0.0);
    world.spawn((Mesh3d(cube.clone()), MeshMaterial3d(half), at(0)));
    world.spawn((Mesh3d(cube.clone()), MeshMaterial3d(red.clone()), MaterialAlpha { alpha: 0.5 }, at(1)));
    world.spawn((Mesh3d(cube), MeshMaterial3d(red), at(2)));
    let ship = world.resource::<AssetServer>().load("models/ship.gltf#Scene0");
    world.spawn((WorldAssetRoot(ship.clone()), MaterialAlpha { alpha: 0.5 }, at(3), Visibility::default()));
    world.spawn((WorldAssetRoot(ship), at(4), Visibility::default()));

    let shot: Arc<Mutex<Option<Image>>> = Arc::default();
    let px = |img: &Image, i: usize| {
        let x = ((XS[i] / 14.7 + 0.5) * W as f32) as usize;
        let at = ((H / 2) as usize * W as usize + x) * 4;
        let d = img.data.as_ref().unwrap();
        [d[at], d[at + 1], d[at + 2]]
    };
    // Red at half alpha over white: full red, green and blue halfway in linear light.
    let pink = |p: [u8; 3]| p[0] > 240 && (150..230).contains(&p[1]) && (150..230).contains(&p[2]);
    let sum = |p: [u8; 3]| p.iter().map(|c| *c as u32).sum::<u32>();
    let mut last = None;
    for frame in 0..900 {
        app.update();
        if frame % 20 == 10 {
            let s = shot.clone();
            app.world_mut().spawn(Screenshot::image(target.clone())).observe(move |e: On<ScreenshotCaptured>| {
                *s.lock().unwrap() = Some(e.image.clone());
            });
        }
        if let Some(img) = shot.lock().unwrap().take() {
            let p: Vec<[u8; 3]> = (0..5).map(|i| px(&img, i)).collect();
            last = Some(p.clone());
            let ships = sum(p[4]) < 600 && sum(p[3]) > sum(p[4]) + 120 && sum(p[3]) < 3 * 250;
            if pink(p[0]) && pink(p[1]) && p[2] == [255, 0, 0] && ships {
                return;
            }
        }
    }
    panic!("never: pink, pink, red, a ship blended over white and one as before: {last:?}");
}
