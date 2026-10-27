//! Shots of a ready run (change `screenshot-ready`): `BEVY_TS_SCREENSHOT`'s shot waits for the
//! script, Bevy's pipeline cache and every asset load, and under the fixed step it is the same at
//! any load, a looping glTF clip included. It waits out a cold build that keeps making progress,
//! and fails, with no file and exit code 1, on a pipeline that never compiles (change
//! `screenshot-cold-and-atomic`).
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
use bevy::render::render_resource::{AsBindGroup, CachedPipelineState, Extent3d, PipelineCache, PipelineDescriptor, TextureDimension, TextureFormat};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::shader::ShaderRef;
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts::generation::{DevSource, ReloadStatus};
use bevy_ts::runtime::RuntimeInfo;
use bevy_ts_cli::compile::Compiler;
use bevy_ts_cli::headless::{self, SIZE};
use bevy_ts_remote::screenshot::{FixedStep, ReadyPlugin, ReadyTimeout, ShotWhenReady, pipelines_waiting};
use common::*;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const EMPTY: Color = Color::srgb(0.0, 0.0, 0.0);
const SCENE: Color = Color::srgb(0.9, 0.3, 0.1);

/// Reads files under a folder, as Bevy's file reader does, each read held until `open` and then
/// `delay` more: a slow disk. A shader `m<N>.wesl` is also held until `shaders` releases `N`.
struct SlowReader {
    files: FileAssetReader,
    open: Arc<AtomicBool>,
    delay: Duration,
    shaders: Gates,
}

/// Shader reads held until released: `m<N>.wesl` waits for gate `N`'s sender to drop. Awaited, not
/// slept: Bevy's IO task pool is one for every app in the process, and a read that sleeps holds
/// one of its few threads, so a few held for ever stop every later test's loads.
#[derive(Clone, Default)]
struct Gates(Arc<Vec<Gate>>);

/// One held read: its sender, dropped to release it, and the receiver a read awaits.
type Gate = (Mutex<Option<async_channel::Sender<()>>>, async_channel::Receiver<()>);

impl Gates {
    /// `count` gates, all held.
    fn held(count: usize) -> Self {
        Self(Arc::new((0..count).map(|_| async_channel::bounded(1)).map(|(tx, rx)| (Mutex::new(Some(tx)), rx)).collect()))
    }

    fn release(&self, n: usize) {
        self.0[n].0.lock().unwrap().take();
    }

    async fn wait(&self, n: usize) {
        if let Some((_, rx)) = self.0.get(n) {
            let _ = rx.recv().await;
        }
    }
}

impl AssetReader for SlowReader {
    async fn read<'a>(&'a self, path: &'a Path) -> Result<impl Reader + 'a, AssetReaderError> {
        if let Some(n) = path.to_str().and_then(|p| p.strip_prefix('m')?.strip_suffix(".wesl")?.parse::<usize>().ok()) {
            self.shaders.wait(n).await;
        }
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
    shaders: Gates,
}

impl Slow {
    /// `root`, read at once but for the shaders `shaders` holds.
    fn shaders(root: PathBuf, shaders: Gates) -> Self {
        Self { root, open: Arc::new(AtomicBool::new(true)), delay: Duration::ZERO, shaders }
    }

    fn open(root: PathBuf, open: Arc<AtomicBool>, delay: Duration) -> Self {
        Self { root, open, delay, shaders: Gates::default() }
    }
}

/// A headless engine with a camera and a script, taking its env shot at the `at`th ready frame
/// into `path`. Pipelines compile in the background, as in a shipped engine. A render that stalls,
/// in a frame or in the app's drop (which waits for the render thread), is the engine's stall
/// watchdog's to report.
fn engine(name: &str, path: &Path, at: u64) -> App {
    engine_with(name, path, at, None)
}

/// As [`engine`], with a `slow://` asset source.
fn engine_with(name: &str, path: &Path, at: u64, slow: Option<Slow>) -> App {
    let dir = project(name, &[("package.json", "{}"), ("game.ts", "import { app } from \"bevy-ts\";\n")]);
    let mut app = render_app();
    if let Some(Slow { root, open, delay, shaders }) = slow {
        app.register_asset_source(
            "slow",
            AssetSourceBuilder::new(move || Box::new(SlowReader { files: FileAssetReader::new(&root), open: open.clone(), delay, shaders: shaders.clone() })),
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
    app
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
    let mut app = engine("ready-late-script", &path, 3);
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
    let mut app = engine("ready-late-material", &path, 3);
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
    let mut app = engine(name, &path, 30);
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

/// `QUADS` copies of one textured quad in one plane facing the camera, each turned about the view
/// axis: every pixel is at the same depth in all of them, so the copy drawn last decides its colour.
/// Shot at the 30th ready frame on the fixed step, with `work` of extra time each frame.
fn stacked_shot(name: &str, work: Duration) -> Vec<u8> {
    const QUADS: usize = 64;
    let dir = shots_dir(name);
    let path = dir.join("stacked.png");
    let mut app = engine(name, &path, 30);
    app.insert_resource(FixedStep)
        .add_systems(Startup, |mut commands: Commands, mut images: ResMut<Assets<Image>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>| {
            let size = 64;
            let texels = (0..size * size).flat_map(|i| [(i % size * 4) as u8, (i / size * 4) as u8, 128, 255]).collect();
            let extent = Extent3d { width: size, height: size, depth_or_array_layers: 1 };
            let texture = images.add(Image::new(extent, TextureDimension::D2, texels, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default()));
            let material = materials.add(StandardMaterial { base_color: Color::WHITE, base_color_texture: Some(texture), unlit: true, ..default() });
            let mesh = meshes.add(Rectangle::new(2.5, 2.5));
            for i in 0..QUADS {
                let turn = Quat::from_rotation_z(i as f32 * std::f32::consts::TAU / QUADS as f32);
                commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::from_rotation(turn)));
            }
        })
        .add_systems(Update, move || std::thread::sleep(work));
    start(&mut app);
    apply_script(&mut app);
    until_shot(&mut app, &path);
    png(&path).data.expect("the shot's pixels")
}

#[test]
fn stacked_instances_at_one_depth_are_the_same_every_shot() {
    let first = stacked_shot("ready-stacked-quick", Duration::ZERO);
    let colours = first.chunks(4).map(|p| [p[0], p[1], p[2]]).collect::<std::collections::HashSet<_>>().len();
    assert!(colours > 100, "the quads' texture is in the shot: {colours} colours");
    for (i, work) in [Duration::from_millis(30), Duration::ZERO, Duration::from_millis(30)].into_iter().enumerate() {
        let again = stacked_shot(&format!("ready-stacked-{i}"), work);
        assert!(first == again, "shot {} differs in {} of {} bytes", i + 2, first.iter().zip(&again).filter(|(a, b)| a != b).count(), first.len());
    }
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
    let slow = Slow::open(root, open.clone(), Duration::ZERO);
    let mut app = engine_with("ready-slow-texture", &path, 3, Some(slow));
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
    let slow = Slow::open(root, Arc::new(AtomicBool::new(true)), delay);
    let mut app = engine_with(name, &path, 30, Some(slow));
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
    let slow = Slow::open(root, open.clone(), Duration::ZERO);
    let mut app = engine_with("ready-method-texture", &dir.join("unused.png"), 3, Some(slow));
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

/// Materials drawing one flat colour each with the fragment shader `slow://m<N>.wesl`: each its own
/// type, so each has its own pipeline.
macro_rules! flat_materials {
    ($($name:ident = $n:literal),*) => {$(
        #[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
        struct $name {}

        impl Material for $name {
            fn fragment_shader() -> ShaderRef {
                concat!("slow://m", $n, ".wesl").into()
            }
        }
    )*};
}

flat_materials!(M0 = 0, M1 = 1, M2 = 2, M3 = 3, M4 = 4, M5 = 5);

/// Writes `m<N>.wesl` for each of `count` flat materials under `root`, each its own colour.
fn flat_shaders(root: &Path, count: usize) {
    std::fs::create_dir_all(root).unwrap();
    for n in 0..count {
        let [r, g, b] = [(n % 2) as f32, (n / 2 % 2) as f32, 0.2 + 0.15 * n as f32];
        let shader = format!(
            "import bevy_pbr::render::forward_io::VertexOutput;\n\n@fragment\nfn fragment(in: VertexOutput) -> @location(0) vec4<f32> {{\n    return vec4<f32>({r:.2}, {g:.2}, {b:.2}, 1.0);\n}}\n"
        );
        std::fs::write(root.join(format!("m{n}.wesl")), shader).unwrap();
    }
}

/// Adds the first `count` flat materials, each on a quad of its own in a 3 x 2 grid.
fn flat_scene(app: &mut App, count: usize) {
    fn add<M: Material<Data: PartialEq + Eq + std::hash::Hash + Clone> + Default>(app: &mut App, n: usize) {
        app.add_plugins(MaterialPlugin::<M>::default()).add_systems(Startup, move |mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<M>>| {
            let at = Vec3::new((n % 3) as f32 - 1.0, 0.5 - (n / 3) as f32, 0.0) * 1.2;
            commands.spawn((Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))), MeshMaterial3d(materials.add(M::default())), Transform::from_translation(at)));
        });
    }
    let adds: [fn(&mut App, usize); 6] = [add::<M0>, add::<M1>, add::<M2>, add::<M3>, add::<M4>, add::<M5>];
    adds.iter().take(count).enumerate().for_each(|(n, add)| add(app, n));
}

/// The fragment shaders of every render pipeline the cache has compiled, recorded each render.
fn shaders_compiled(app: &mut App) -> Arc<Mutex<HashSet<AssetId<Shader>>>> {
    let compiled = Arc::new(Mutex::new(HashSet::new()));
    let out = compiled.clone();
    app.sub_app_mut(RenderApp).add_systems(
        Render,
        (move |cache: Res<PipelineCache>| {
            let mut out = out.lock().unwrap();
            for pipeline in cache.pipelines() {
                if let (PipelineDescriptor::RenderPipelineDescriptor(d), CachedPipelineState::Ok(_)) = (&pipeline.descriptor, &pipeline.state)
                    && let Some(fragment) = &d.fragment
                {
                    out.insert(fragment.shader.id());
                }
            }
        })
        .in_set(RenderSystems::Cleanup),
    );
    compiled
}

/// The six flat materials shot at the 30th ready frame on the fixed step, their shaders read one
/// at a time: each `gap` after the last one's pipeline compiled. A zero `gap` holds none.
fn cold_build_shot(name: &str, window: Duration, gap: Option<Duration>) -> (Vec<u8>, Duration) {
    let dir = shots_dir(name);
    let path = dir.join("cold.png");
    flat_shaders(&dir.join("assets"), 6);
    let gates = Gates::held(6);
    let mut released = if gap.is_some() { 1 } else { 6 };
    (0..released).for_each(|n| gates.release(n));
    let mut app = engine_with(name, &path, 30, Some(Slow::shaders(dir.join("assets"), gates.clone())));
    app.insert_resource(FixedStep);
    flat_scene(&mut app, 6);
    let compiled = shaders_compiled(&mut app);
    start(&mut app);
    apply_script(&mut app);
    let began = Instant::now();
    let mut seen: Option<Instant> = None;
    until(&mut app, &format!("{} written", path.display()), |app| {
        assert!(app.should_exit().is_none(), "the shot gave up while pipelines compiled");
        window_once_rendered(app, window);
        let n = released;
        if let Some(gap) = gap && n < 6 {
            assert!(!path.exists(), "shot with shader {n} still held");
            let shader = app.world().resource::<AssetServer>().get_handle::<Shader>(format!("slow://m{}.wesl", n - 1));
            let done = shader.is_some_and(|s| compiled.lock().unwrap().contains(&s.id()));
            if done && seen.get_or_insert_with(Instant::now).elapsed() >= gap {
                gates.release(n);
                released += 1;
                seen = None;
            }
        }
        std::fs::metadata(&path).is_ok_and(|m| m.len() > 0) && !app.world().contains_resource::<ShotWhenReady>()
    });
    (png(&path).data.expect("the shot's pixels"), began.elapsed())
}

/// Sets `ReadyTimeout` to `window` once a frame has rendered: the first render, under load, takes
/// longer than a short window, and it is the build after it a test times.
fn window_once_rendered(app: &mut App, window: Duration) {
    if pipelines_waiting(app.world()).is_some() && app.world().resource::<ReadyTimeout>().0 != window {
        app.insert_resource(ReadyTimeout(window));
    }
}

/// Six pipelines compiled one at a time, over longer than the no-progress window but never with a
/// gap as long as it: the shot waits them out, and is the shot of the same scene with nothing held.
#[test]
fn a_cold_build_longer_than_the_limit_is_waited_out() {
    let window = Duration::from_secs(5);
    let (cold, took) = cold_build_shot("ready-cold-held", window, Some(Duration::from_millis(1250)));
    assert!(took > window, "the build took {took:?}, not over the window");
    let (warm, _) = cold_build_shot("ready-cold-free", window, None);
    let colours = warm.chunks(4).map(|p| [p[0], p[1], p[2]]).collect::<HashSet<_>>().len();
    assert!(colours >= 7, "six materials and the clear colour: {colours} colours");
    assert!(cold == warm, "the two shots differ in {} of {} bytes", cold.iter().zip(&warm).filter(|(a, b)| a != b).count(), cold.len());
}

/// A shader read held for ever: the env shot saves nothing and exits 1, and `bevy-ts/screenshot`
/// in the same app fails naming its path and the pipelines waiting.
#[test]
fn a_pipeline_that_never_compiles_fails_the_env_shot() {
    let dir = shots_dir("ready-never-compiles");
    let (env, method) = (dir.join("env.png"), dir.join("method.png"));
    flat_shaders(&dir.join("assets"), 1);
    let slow = Slow::shaders(dir.join("assets"), Gates::held(1));
    let mut app = engine_with("ready-never-compiles", &env, 3, Some(slow));
    app.add_plugins(RemotePlugin::default());
    flat_scene(&mut app, 1);
    bevy_ts_remote::add_methods(&mut app);
    start(&mut app);
    apply_script(&mut app);
    // A frame rendered first, then a short window: what times out is the pipeline, not the start.
    until(&mut app, "a frame rendered", |app| {
        window_once_rendered(app, Duration::from_secs(1));
        app.world().resource::<ReadyTimeout>().0 == Duration::from_secs(1)
    });
    let (tx, rx) = async_channel::bounded(1);
    let message = BrpMessage { method: "bevy-ts/screenshot".into(), params: Some(json!({ "path": method })), sender: tx };
    app.world().resource::<BrpSender>().try_send(message).unwrap();
    let (mut exit, mut answer) = (None, None);
    until(&mut app, "the env shot's exit and the method's answer", |app| {
        if exit.is_none() && !app.world().contains_resource::<ShotWhenReady>() {
            exit = Some(app.should_exit());
        }
        answer = answer.take().or_else(|| rx.try_recv().ok());
        exit.is_some() && answer.is_some()
    });
    assert_eq!(exit.unwrap(), Some(AppExit::from_code(1)), "the env shot's exit");
    assert!(!env.exists(), "the env shot saved a file");
    let error = answer.unwrap().expect_err("the method failed").message;
    assert!(error.contains(&method.display().to_string()) && error.contains("pipelines still waiting"), "the method's error: {error}");
    assert!(!method.exists(), "the method saved a file");
    let left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).filter(|n| n != "assets").collect();
    assert!(left.is_empty(), "files left: {left:?}");
}
