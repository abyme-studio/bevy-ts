//! Shots of a ready run (change `screenshot-ready`): `BEVY_TS_SCREENSHOT`'s shot waits for the
//! script and for Bevy's pipeline cache, and under the fixed step it is the same at any load.
//! macOS (Metal), where pipelines compile in the background.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts::runtime::RuntimeInfo;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless::{self, SIZE};
use bevy_ts_remote::screenshot::{FixedStep, ReadyPlugin, ShotWhenReady};
use common::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const EMPTY: Color = Color::srgb(0.0, 0.0, 0.0);
const SCENE: Color = Color::srgb(0.9, 0.3, 0.1);

/// A headless engine with a camera and a script, taking its env shot at the `at`th ready frame
/// into `path`. Pipelines compile in the background, as in a shipped engine.
fn engine(name: &str, path: &Path, at: u64) -> (PathBuf, App) {
    let dir = project(name, &[("package.json", "{}"), ("game.ts", "import { app } from \"bevy-ts\";\n")]);
    let mut app = render_app();
    app.add_plugins(headless::default_plugins(true))
        .add_plugins(headless::HeadlessPlugin)
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
        .add_plugins(ReadyPlugin)
        .insert_resource(ClearColor(EMPTY))
        .insert_resource(ShotWhenReady { at, ..ShotWhenReady::new(path) })
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 0.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y)));
            commands.spawn((DirectionalLight::default(), Transform::from_xyz(1.0, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y)));
        });
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.ts"), true).unwrap())));
    (dir, app)
}

fn start(app: &mut App) {
    app.finish();
    app.cleanup();
}

fn apply_script(app: &mut App) {
    let r = reload(app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
}

/// Updates until the shot is written.
fn until_shot(app: &mut App, path: &Path) {
    for _ in 0..1200 {
        app.update();
        if path.exists() && std::fs::metadata(path).is_ok_and(|m| m.len() > 0) && !app.world().contains_resource::<ShotWhenReady>() {
            // `save_to_disk` writes from a task; one more update lets it finish.
            for _ in 0..5 {
                app.update();
            }
            return;
        }
    }
    panic!("{} was never written", path.display());
}

fn png(path: &Path) -> Image {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Image::from_buffer(&bytes, ImageType::Extension("png"), CompressedImageFormats::NONE, true, ImageSampler::Default, RenderAssetUsages::default()).unwrap()
}

fn centre(path: &Path) -> Color {
    let image = png(path);
    assert_eq!((image.width(), image.height()), SIZE);
    image.get_color_at(SIZE.0 / 2, SIZE.1 / 2).unwrap()
}

fn shots_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("bevy-ts-cli-tests").join(format!("{name}-shots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// A cube filling the view, in an unlit `SCENE` colour.
fn cube(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let material = materials.add(StandardMaterial { base_color: SCENE, unlit: true, ..default() });
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(3.0, 3.0, 0.5))), MeshMaterial3d(material)));
}

#[test]
fn a_script_that_starts_late_is_in_the_shot() {
    let dir = shots_dir("ready-late-script");
    let path = dir.join("late.png");
    let (_, mut app) = engine("ready-late-script", &path, 3);
    // The script's scene: the cube, spawned once its first generation has applied.
    app.add_systems(Update, |info: Res<RuntimeInfo>, mut done: Local<bool>, commands: Commands, meshes: ResMut<Assets<Mesh>>, materials: ResMut<Assets<StandardMaterial>>| {
        if info.generation > 0 && !*done {
            *done = true;
            cube(commands, meshes, materials);
        }
    });
    start(&mut app);
    for _ in 0..100 {
        app.update();
    }
    assert!(!path.exists(), "shot before the script started");
    apply_script(&mut app);
    until_shot(&mut app, &path);
    let pixel = centre(&path);
    assert!((pixel.to_srgba().to_vec3() - SCENE.to_srgba().to_vec3()).length() < 0.05, "the script's cube, not {pixel:?}");
}

#[test]
fn a_material_compiled_in_the_background_is_in_the_shot() {
    let dir = shots_dir("ready-late-material");
    let path = dir.join("material.png");
    let (_, mut app) = engine("ready-late-material", &path, 3);
    app.add_systems(Startup, cube);
    start(&mut app);
    apply_script(&mut app);
    until_shot(&mut app, &path);
    let pixel = centre(&path);
    assert!((pixel.to_srgba().to_vec3() - SCENE.to_srgba().to_vec3()).length() < 0.05, "the cube's material, not {pixel:?}");
}

/// The cube turned by elapsed time, shot at the 30th ready frame on the fixed step, with `work` of
/// extra time each frame.
fn turning_shot(name: &str, work: std::time::Duration) -> Vec<u8> {
    let dir = shots_dir(name);
    let path = dir.join("turn.png");
    let (_, mut app) = engine(name, &path, 30);
    app.insert_resource(FixedStep)
        .add_systems(Startup, cube)
        .add_systems(Update, move |time: Res<Time>, mut cubes: Query<&mut Transform, With<Mesh3d>>| {
            std::thread::sleep(work);
            for mut t in &mut cubes {
                t.rotation = Quat::from_rotation_y(time.elapsed_secs() * 2.0) * Quat::from_rotation_x(time.elapsed_secs());
            }
        });
    start(&mut app);
    apply_script(&mut app);
    until_shot(&mut app, &path);
    png(&path).data.expect("the shot's pixels")
}

#[test]
fn the_fixed_step_shot_is_the_same_at_any_load() {
    let quick = turning_shot("ready-fixed-quick", std::time::Duration::ZERO);
    let loaded = turning_shot("ready-fixed-loaded", std::time::Duration::from_millis(30));
    assert!(quick == loaded, "the two shots differ in {} of {} bytes", quick.iter().zip(&loaded).filter(|(a, b)| a != b).count(), quick.len());
}
