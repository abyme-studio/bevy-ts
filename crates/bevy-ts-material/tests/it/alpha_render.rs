//! `MaterialAlpha` drawn (changes `material-alpha`, `material-fade`), into an offscreen image with
//! `Msaa::Off`: a faded target dithers, so a model fades as one solid, and its shadow follows its
//! alpha. Things are found on screen by projecting where they stand through the camera.

use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_material::{MaterialAlpha, MaterialAlphaTargets, ScriptMaterialRenderPlugin};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const W: u32 = 320;
const H: u32 = 180;

/// One render app at a time: each holds a GPU device and compiles pipelines.
static GPU: Mutex<()> = Mutex::new(());

fn app() -> App {
    let mut app = App::new();
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets");
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: assets.to_string_lossy().into_owned(), watch_for_changes_override: Some(false), ..default() }),
    )
    .add_plugins(ScriptMaterialRenderPlugin);
    while app.plugins_state() == bevy::app::PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    app
}

/// A camera at `at` drawing into a new image over white; the image and the camera.
fn camera(world: &mut World, at: Transform) -> (Handle<Image>, Entity) {
    let mut target = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    let cam = world
        .spawn((
            Camera3d::default(),
            Camera { clear_color: ClearColorConfig::Custom(Color::WHITE), ..default() },
            Tonemapping::None,
            Msaa::Off,
            RenderTarget::Image(target.clone().into()),
            at,
        ))
        .id();
    (target, cam)
}

/// Screenshots of `target` until `check` passes on one. Pipelines and assets finish on other
/// threads, which a loaded machine slows by any amount, so the bound is a long wall-clock one that
/// names what never came.
fn until_drawn(app: &mut App, target: &Handle<Image>, mut check: impl FnMut(&World, &Image) -> Result<(), String>) {
    let bound = Duration::from_secs(180);
    let start = Instant::now();
    let shot: Arc<Mutex<Option<Image>>> = Arc::default();
    let mut pending = false;
    let mut last = String::from("no screenshot");
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
            match check(app.world(), &img) {
                Ok(()) => return,
                Err(e) => last = e,
            }
        }
        assert!(start.elapsed() < bound, "never within {bound:?}: {last}");
    }
}

/// A screen rectangle, `min` inclusive, `max` exclusive.
#[derive(Clone, Copy, Debug)]
struct Rect2 {
    min: UVec2,
    max: UVec2,
}

/// The screen box of world points `a` and `b`, shrunk by `inset` pixels on each side.
fn project(world: &World, cam: Entity, a: Vec3, b: Vec3, inset: u32) -> Result<Rect2, String> {
    let (c, t) = (world.get::<Camera>(cam).unwrap(), world.get::<GlobalTransform>(cam).unwrap());
    let p = |v| c.world_to_viewport(t, v).map_err(|e| format!("{e:?}"));
    let (pa, pb) = (p(a)?, p(b)?);
    let (lo, hi) = (pa.min(pb), pa.max(pb));
    let min = lo.as_uvec2() + UVec2::splat(inset);
    let max = hi.as_uvec2().min(UVec2::new(W, H)).saturating_sub(UVec2::splat(inset));
    if min.x >= max.x || min.y >= max.y {
        return Err(format!("{a} to {b} is off screen or too small: {lo} {hi}"));
    }
    Ok(Rect2 { min, max })
}

fn pixels(img: &Image, r: Rect2) -> impl Iterator<Item = [u8; 3]> + '_ {
    let d = img.data.as_ref().unwrap();
    (r.min.y..r.max.y).flat_map(move |y| {
        (r.min.x..r.max.x).map(move |x| {
            let at = ((y * W + x) * 4) as usize;
            [d[at], d[at + 1], d[at + 2]]
        })
    })
}

fn fraction(img: &Image, r: Rect2, f: impl Fn([u8; 3]) -> bool) -> f32 {
    let (hit, all) = pixels(img, r).fold((0, 0), |(h, a), p| (h + f(p) as u32, a + 1));
    hit as f32 / all as f32
}

/// Mean brightness, 0..255.
fn mean(img: &Image, r: Rect2) -> f32 {
    let (sum, n) = pixels(img, r).fold((0u32, 0u32), |(s, n), p| (s + p.iter().map(|c| *c as u32).sum::<u32>(), n + 1));
    sum as f32 / n as f32 / 3.0
}

fn red(p: [u8; 3]) -> bool {
    p[0] > 200 && p[1] < 60 && p[2] < 60
}

fn white(p: [u8; 3]) -> bool {
    p.iter().all(|c| *c > 245)
}

/// Left to right over white, unlit: a red cube built `<material color="#ff0000" alpha={0.5}>` (it
/// blends, as before), a red cube with `MaterialAlpha(0.5)`, its twin with none, `ship.gltf` under
/// a root with `MaterialAlpha(0.5)`, and a second ship with none.
#[test]
fn faded_shapes_and_scenes_dither_and_their_twins_do_not() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let world = app.world_mut();
    let (target, cam) = camera(world, Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y));
    const XS: [f32; 5] = [-5.0, -2.5, 0.0, 2.5, 5.0];
    let cube = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(1.5, 1.5, 1.5));
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let half = materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.0, 0.0, 0.5), alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
    let solid = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.0, 0.0), unlit: true, ..default() });
    let at = |i: usize| Transform::from_xyz(XS[i], 0.0, 0.0);
    world.spawn((Mesh3d(cube.clone()), MeshMaterial3d(half), at(0)));
    world.spawn((Mesh3d(cube.clone()), MeshMaterial3d(solid.clone()), MaterialAlpha { alpha: 0.5 }, at(1)));
    world.spawn((Mesh3d(cube), MeshMaterial3d(solid), at(2)));
    let ship = world.resource::<AssetServer>().load("models/ship.gltf#Scene0");
    let faded_ship = world.spawn((WorldAssetRoot(ship.clone()), MaterialAlpha { alpha: 0.5 }, at(3), Visibility::default())).id();
    world.spawn((WorldAssetRoot(ship), at(4), Visibility::default()));

    // Red at half alpha over white: full red, green and blue halfway in linear light.
    let pink = |p: [u8; 3]| p[0] > 240 && (150..230).contains(&p[1]) && (150..230).contains(&p[2]);
    until_drawn(&mut app, &target, |w, img| {
        let face = |i: usize| project(w, cam, Vec3::new(XS[i] - 0.75, -0.75, 0.75), Vec3::new(XS[i] + 0.75, 0.75, 0.75), 2);
        let ship = |i: usize| project(w, cam, Vec3::new(XS[i] - 1.0, -1.0, 0.5), Vec3::new(XS[i] + 1.0, 2.0, 0.5), 0);
        let blended = fraction(img, face(0)?, pink);
        let dithered = (fraction(img, face(1)?, red), fraction(img, face(1)?, white));
        let twin = fraction(img, face(2)?, red);
        let ships = (fraction(img, ship(3)?, |p| !white(p)), fraction(img, ship(4)?, |p| !white(p)));
        let targets = w.get::<MaterialAlphaTargets>(faded_ship).map_or(0, |t| t.targets.len());
        let ok = blended > 0.99
            && (dithered.0 - 0.5).abs() <= 1.0 / 16.0
            && dithered.0 + dithered.1 > 0.99
            && twin > 0.99
            && targets == 3
            && ships.1 > 0.1
            && (ships.0 / ships.1 - 0.5).abs() <= 0.1;
        if ok { Ok(()) } else { Err(format!("pink {blended}, dithered red/white {dithered:?}, twin {twin}, ship targets {targets}, ships' drawn fraction {ships:?}")) }
    });
}

/// A red cube holding a black one, both of `nested.gltf`, under a root at 0.5, unlit over white:
/// the inner cube never shows, and half the red pixels draw.
#[test]
fn a_faded_model_draws_as_one_solid() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let world = app.world_mut();
    let (target, cam) = camera(world, Transform::from_xyz(2.0, 1.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y));
    let model = world.resource::<AssetServer>().load("models/nested.gltf#Scene0");
    let root = world.spawn((WorldAssetRoot(model), MaterialAlpha { alpha: 0.5 }, Transform::default(), Visibility::default())).id();

    until_drawn(&mut app, &target, |w, img| {
        let face = project(w, cam, Vec3::new(-0.5, -0.5, 0.5), Vec3::new(0.5, 0.5, 0.5), 3)?;
        let targets = w.get::<MaterialAlphaTargets>(root).map_or(0, |t| t.targets.len());
        let black = fraction(img, face, |p| p.iter().map(|c| *c as u32).sum::<u32>() < 150);
        let r = fraction(img, face, red);
        if targets == 2 && black == 0.0 && (r - 0.5).abs() <= 1.0 / 16.0 { Ok(()) } else { Err(format!("targets {targets}, near-black {black}, red {r}")) }
    });
}

/// Lit from the +z side so each shadow falls 1.5 behind what casts it, seen from above. Left to
/// right: red cubes at `MaterialAlpha` 1, 0.5 and 0, `ship.gltf` at 0, and a cube with
/// `NotShadowCaster` at 1.
#[test]
fn a_faded_targets_shadow_follows_its_alpha() {
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = app();
    let world = app.world_mut();
    let (target, cam) = camera(world, Transform::from_xyz(0.0, 18.0, -1.0).looking_at(Vec3::new(0.0, 0.0, -1.0), Vec3::NEG_Z));
    world.spawn((
        DirectionalLight { illuminance: 2000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(0.0, 1.0, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder { num_cascades: 1, maximum_distance: 40.0, first_cascade_far_bound: 40.0, ..default() }.build(),
    ));
    let mut meshes = world.resource_mut::<Assets<Mesh>>();
    let ground = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(20.0)));
    let cube = meshes.add(Cuboid::default());
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let white = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 1.0, ..default() });
    let solid = materials.add(StandardMaterial::from(Color::srgb(1.0, 0.0, 0.0)));
    world.spawn((Mesh3d(ground), MeshMaterial3d(white)));
    const XS: [f32; 5] = [-6.0, -3.0, 0.0, 3.0, 6.0];
    const UP: f32 = 1.5;
    let at = |i: usize| Transform::from_xyz(XS[i], UP, 0.0);
    world.spawn((Mesh3d(cube.clone()), MeshMaterial3d(solid.clone()), MaterialAlpha { alpha: 1.0 }, at(0)));
    world.spawn((Mesh3d(cube.clone()), MeshMaterial3d(solid.clone()), MaterialAlpha { alpha: 0.5 }, at(1)));
    world.spawn((Mesh3d(cube.clone()), MeshMaterial3d(solid.clone()), MaterialAlpha { alpha: 0.0 }, at(2)));
    let ship = world.resource::<AssetServer>().load("models/ship.gltf#Scene0");
    let ship = world.spawn((WorldAssetRoot(ship), MaterialAlpha { alpha: 0.0 }, at(3), Visibility::default())).id();
    world.spawn((Mesh3d(cube), MeshMaterial3d(solid), NotShadowCaster, MaterialAlpha { alpha: 1.0 }, at(4)));

    until_drawn(&mut app, &target, |w, img| {
        // The ground where the body of each would cast, and a bare patch beside them.
        let under = |x: f32| project(w, cam, Vec3::new(x - 0.25, 0.0, -UP - 0.25), Vec3::new(x + 0.25, 0.0, -UP + 0.25), 0).map(|r| mean(img, r));
        let s: Vec<f32> = XS.iter().map(|x| under(*x)).collect::<Result<_, _>>()?;
        let bare = under(-9.0)?;
        let targets = w.get::<MaterialAlphaTargets>(ship).map_or(0, |t| t.targets.len());
        let same = |v: f32| (v - bare).abs() <= 3.0;
        let ok = targets == 3 && s[0] < bare - 30.0 && s[1] < bare - 10.0 && s[1] > s[0] + 10.0 && same(s[2]) && same(s[3]) && same(s[4]);
        if ok { Ok(()) } else { Err(format!("ship targets {targets}, ground under 1, 0.5, 0, ship 0, NotShadowCaster: {s:?}, bare {bare}")) }
    });
}
