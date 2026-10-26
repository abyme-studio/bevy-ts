//! Shots of a ready run (change `screenshot-ready`): `BEVY_TS_SCREENSHOT`'s shot waits for the
//! script, Bevy's pipeline cache and every asset load, and under the fixed step it is the same at
//! any load, a looping glTF clip included.
//! macOS (Metal), where pipelines compile in the background.
#![cfg(target_os = "macos")]

use crate::common;

use bevy::asset::RenderAssetUsages;
use bevy::asset::io::file::FileAssetReader;
use bevy::asset::io::{AssetReader, AssetReaderError, AssetSourceBuilder, PathStream, Reader};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::remote::{BrpMessage, BrpSender, RemotePlugin};
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts::runtime::RuntimeInfo;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless::{self, SIZE};
use bevy_ts_remote::screenshot::{FixedStep, ReadyPlugin, ShotWhenReady};
use common::*;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const EMPTY: Color = Color::srgb(0.0, 0.0, 0.0);
const SCENE: Color = Color::srgb(0.9, 0.3, 0.1);

/// Reads files under a folder, as Bevy's file reader does, each read held until `open` and then
/// `delay` more: a slow disk.
struct SlowReader {
    files: FileAssetReader,
    open: Arc<AtomicBool>,
    delay: Duration,
}

impl AssetReader for SlowReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        while !self.open.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(self.delay);
        self.files.read(path).await
    }
    async fn read_meta<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        self.files.read_meta(path).await
    }
    async fn read_directory<'a>(&'a self, path: &'a Path) -> Result<Box<PathStream>, AssetReaderError> {
        self.files.read_directory(path).await
    }
    async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
        self.files.is_directory(path).await
    }
}

/// The `slow://` asset source: `root` read through a [`SlowReader`].
struct Slow {
    root: PathBuf,
    open: Arc<AtomicBool>,
    delay: Duration,
}

/// A headless engine with a camera and a script, taking its env shot at the `at`th ready frame
/// into `path`, and its watchdog. Pipelines compile in the background, as in a shipped engine.
/// Bind the watchdog before the app (`let (_hang, mut app)`) so it drops after: dropping a Bevy
/// app waits for its render thread, and a stalled one hangs the drop.
fn engine(name: &str, path: &Path, at: u64) -> (Watchdog, App) {
    engine_with(name, path, at, None)
}

/// As [`engine`], with a `slow://` asset source.
fn engine_with(name: &str, path: &Path, at: u64, slow: Option<Slow>) -> (Watchdog, App) {
    let dir = project(name, &[("package.json", "{}"), ("game.ts", "import { app } from \"bevy-ts\";\n")]);
    let mut app = render_app();
    // Armed once the render lock is held, so time queued behind other render tests does not count.
    let hang = hang(name);
    if let Some(Slow { root, open, delay }) = slow {
        app.register_asset_source(
            "slow",
            AssetSourceBuilder::new(move || Box::new(SlowReader { files: FileAssetReader::new(&root), open: open.clone(), delay })),
        );
    }
    app.add_plugins(headless::default_plugins(true))
        .add_plugins(headless::HeadlessPlugin)
        .add_plugins(bevy_ts::BevyTsPlugin::new(bevy_ts_v8::factory(Default::default())).dev(true))
        .add_plugins(ReadyPlugin)
        .insert_resource(ClearColor(EMPTY))
        .insert_resource({ let mut shot = ShotWhenReady::new(path); shot.at = at; shot })
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn((Camera3d::default(), Tonemapping::None, Transform::from_xyz(0.0, 0.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y)));
            commands.spawn((DirectionalLight::default(), Transform::from_xyz(1.0, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y)));
        });
    app.insert_resource(DevSource(Arc::new(Compiler::new(dir.join("game.ts"), true).unwrap())));
    (hang, app)
}

fn start(app: &mut App) {
    app.finish();
    app.cleanup();
}

fn apply_script(app: &mut App) {
    let r = reload(app, false);
    assert_eq!(r.status, ReloadStatus::Applied, "{:?}", r.errors);
}

/// Longer than a ready test takes at load 40: a shot may wait out the method's own timeouts,
/// `READY_TIMEOUT` and then `READBACK_TIMEOUT`, at 400 ms a frame. 120 s fired on runs that answered.
const HANG: Duration = Duration::from_secs(300);

/// Fails the run if the test is still going after [`HANG`]. Every frame submits to the GPU, and a
/// stalled Metal driver blocks inside wgpu's submit, before any ready wait can time out, or in the
/// app's drop, which waits for the render thread.
fn hang(test: &str) -> Watchdog {
    let name: &'static str = Box::leak(format!("{test} (a GPU or driver stall inside wgpu submit is the suspect)").into_boxed_str());
    watchdog(name, HANG)
}

/// Updates until the shot is written.
fn until_shot(app: &mut App, path: &Path) {
    until(app, &format!("{} written", path.display()), |app| {
        std::fs::metadata(path).is_ok_and(|m| m.len() > 0) && !app.world().contains_resource::<ShotWhenReady>()
    });
    // `save_to_disk` writes from a task; one more update lets it finish.
    for _ in 0..5 {
        app.update();
    }
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

/// A cube filling the view, in an unlit `SCENE` colour (the camera does no tonemapping).
fn cube(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let material = materials.add(StandardMaterial { base_color: SCENE, unlit: true, ..default() });
    commands.spawn((Mesh3d(meshes.add(Cuboid::new(3.0, 3.0, 0.5))), MeshMaterial3d(material)));
}

#[test]
fn a_script_that_starts_late_is_in_the_shot() {
    let dir = shots_dir("ready-late-script");
    let path = dir.join("late.png");
    let (_hang, mut app) = engine("ready-late-script", &path, 3);
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
    let (_hang, mut app) = engine("ready-late-material", &path, 3);
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
    let (_hang, mut app) = engine(name, &path, 30);
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

/// A PNG of 4 x 4 RGBA pixels, one colour.
fn png_of(rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, 4, 4);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgba.repeat(16)).unwrap();
    out
}

#[test]
fn a_texture_on_a_slow_disk_is_in_the_shot() {
    let dir = shots_dir("ready-slow-texture");
    let path = dir.join("texture.png");
    let root = dir.join("assets");
    std::fs::create_dir_all(&root).unwrap();
    let [r, g, b, _] = SCENE.to_srgba().to_u8_array();
    std::fs::write(root.join("scene.png"), png_of([r, g, b, 255])).unwrap();
    let open = Arc::new(AtomicBool::new(false));
    let slow = Slow { root, open: open.clone(), delay: Duration::ZERO };
    let (_hang, mut app) = engine_with("ready-slow-texture", &path, 3, Some(slow));
    // A white cube whose colour is its texture, added without the server, as a script's is.
    app.add_systems(Startup, |mut commands: Commands, server: Res<AssetServer>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>| {
        let texture = server.load("slow://scene.png");
        let material = materials.add(StandardMaterial { base_color: Color::WHITE, base_color_texture: Some(texture), unlit: true, ..default() });
        commands.spawn((Mesh3d(meshes.add(Cuboid::new(3.0, 3.0, 0.5))), MeshMaterial3d(material)));
    });
    start(&mut app);
    apply_script(&mut app);
    for _ in 0..100 {
        app.update();
    }
    assert!(!path.exists(), "shot while the texture was still loading");
    open.store(true, Ordering::Relaxed);
    until_shot(&mut app, &path);
    let pixel = centre(&path);
    assert!((pixel.to_srgba().to_vec3() - SCENE.to_srgba().to_vec3()).length() < 0.05, "the texture, not {pixel:?}");
}

/// Bevy's fox from a slow disk, its run clip looping, shot at the 30th ready frame on the fixed
/// step: its glTF read after `delay`, with `work` of extra time each frame.
fn fox_shot(name: &str, delay: Duration, work: Duration) -> Vec<u8> {
    let dir = shots_dir(name);
    let path = dir.join("fox.png");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/foxes/assets/models").canonicalize().unwrap();
    let slow = Slow { root, open: Arc::new(AtomicBool::new(true)), delay };
    let (_hang, mut app) = engine_with(name, &path, 30, Some(slow));
    app.add_plugins(bevy_ts_animation::PlayAnimationPlugin)
        .insert_resource(FixedStep)
        .add_systems(Startup, |mut commands: Commands, server: Res<AssetServer>| {
            commands.spawn((
                WorldAssetRoot(server.load("slow://Fox.glb#Scene0")),
                bevy_ts_animation::PlayAnimation { clip: server.load("slow://Fox.glb#Animation2"), ..default() },
                Transform::from_xyz(0.0, -1.2, 0.0).with_scale(Vec3::splat(0.03)).with_rotation(Quat::from_rotation_y(1.2)),
            ));
        })
        .add_systems(Update, move || std::thread::sleep(work));
    start(&mut app);
    apply_script(&mut app);
    until_shot(&mut app, &path);
    png(&path).data.expect("the shot's pixels")
}

#[test]
fn a_looping_clip_on_a_slow_gltf_is_the_same_at_any_load() {
    let quick = fox_shot("ready-fox-quick", Duration::ZERO, Duration::ZERO);
    let loaded = fox_shot("ready-fox-loaded", Duration::from_millis(500), Duration::from_millis(30));
    let lit = quick.chunks(4).filter(|p| p[..3] != [0, 0, 0]).count();
    assert!(lit > 1000, "the fox is in the shot: {lit} pixels");
    assert!(quick == loaded, "the two shots differ in {} of {} bytes", quick.iter().zip(&loaded).filter(|(a, b)| a != b).count(), quick.len());
}

/// `bevy-ts/screenshot` waits as the env shot does: asked while a texture's read is held, it
/// answers only after the read, and its PNG shows the texture.
#[test]
fn the_method_waits_for_a_texture_on_a_slow_disk() {
    let dir = shots_dir("ready-method-texture");
    let path = dir.join("method.png");
    let root = dir.join("assets");
    std::fs::create_dir_all(&root).unwrap();
    let [r, g, b, _] = SCENE.to_srgba().to_u8_array();
    std::fs::write(root.join("scene.png"), png_of([r, g, b, 255])).unwrap();
    let open = Arc::new(AtomicBool::new(false));
    let slow = Slow { root, open: open.clone(), delay: Duration::ZERO };
    let (_hang, mut app) = engine_with("ready-method-texture", &dir.join("unused.png"), 3, Some(slow));
    app.world_mut().remove_resource::<ShotWhenReady>();
    app.add_plugins(RemotePlugin::default())
        .add_systems(Startup, |mut commands: Commands, server: Res<AssetServer>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>| {
            let texture = server.load("slow://scene.png");
            let material = materials.add(StandardMaterial { base_color: Color::WHITE, base_color_texture: Some(texture), unlit: true, ..default() });
            commands.spawn((Mesh3d(meshes.add(Cuboid::new(3.0, 3.0, 0.5))), MeshMaterial3d(material)));
        });
    bevy_ts_remote::add_methods(&mut app);
    start(&mut app);
    apply_script(&mut app);
    // `RemotePlugin` makes its mailbox in `PreStartup`.
    app.update();
    let (tx, rx) = async_channel::bounded(1);
    let message = BrpMessage { method: "bevy-ts/screenshot".into(), params: Some(json!({ "path": path })), sender: tx };
    app.world().resource::<BrpSender>().try_send(message).unwrap();
    for _ in 0..100 {
        app.update();
        assert!(rx.try_recv().is_err(), "answered while the texture was still loading");
    }
    assert!(!path.exists(), "shot while the texture was still loading");
    open.store(true, Ordering::Relaxed);
    let mut answer = None;
    until(&mut app, "the method's answer", |_| {
        answer = rx.try_recv().ok();
        answer.is_some()
    });
    let answer = answer.unwrap().unwrap();
    assert_eq!(answer["path"], json!(path));
    let pixel = centre(&path);
    assert!((pixel.to_srgba().to_vec3() - SCENE.to_srgba().to_vec3()).length() < 0.05, "the texture, not {pixel:?}");
}
