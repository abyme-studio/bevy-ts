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
use bevy_ts_remote::order_dump::OrderDump;
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
        let gated = |prefix: char, ext: &str| path.to_str().and_then(|p| p.strip_prefix(prefix)?.strip_suffix(ext)?.parse::<usize>().ok());
        if let Some(n) = gated('m', ".wesl").or_else(|| gated('g', ".gltf")) {
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
fn engine(name: &str, path: &Path, at: u64) -> (common::TestDir, App) {
    engine_with(name, path, at, None)
}

/// As [`engine`], with a `slow://` asset source.
fn engine_with(name: &str, path: &Path, at: u64, slow: Option<Slow>) -> (common::TestDir, App) {
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
    app.insert_resource(dir.clone());
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

fn shots_dir(name: &str) -> common::TestDir {
    common::TestDir::new(&format!("{name}-shots"))
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
    let (_game, mut app) = engine("ready-late-script", &path, 3);
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
    let (_game, mut app) = engine("ready-late-material", &path, 3);
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
    let (_game, mut app) = engine(name, &path, 30);
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
    let (_game, mut app) = engine(name, &path, 30);
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

/// Two bins, their instances queued in a scrambled order and keyed in another, as a frame whose
/// assets loaded in another order does: with Bevy's [`StableBinOrder`], which [`ReadyPlugin`] fills
/// under the fixed step, bins draw in the order of their first key and each bin's instances in key
/// order; without it, bins in bin key order and instances in queue order. The same for batchable
/// bins and unbatchable ones (a skinned mesh's).
#[test]
fn bins_draw_in_key_order_with_stable_bin_order() {
    use bevy::asset::uuid::Uuid;
    use bevy::core_pipeline::core_3d::{Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey};
    use bevy::ecs::system::RunSystemOnce;
    use bevy::render::batching::gpu_preprocessing::GpuPreprocessingMode;
    use bevy::render::batching::sort_binned_render_phase;
    use bevy::render::mesh::allocator::MeshSlabs;
    use bevy::render::render_phase::{BinnedRenderPhaseType, DrawFunctionId, InputUniformIndex, StableBinOrder, ViewBinnedRenderPhases};
    use bevy::render::render_resource::CachedRenderPipelineId;
    use bevy::render::sync_world::MainEntity;
    use bevy::render::view::RetainedViewEntity;

    let bin = |n: u128| Opaque3dBinKey { asset_id: AssetId::<Mesh>::Uuid { uuid: Uuid::from_u128(n) }.untyped() };
    // (bin, queue order, key): bin 1 sorts first by its key, bin 2 by its smallest instance key.
    let queued = [(1, 30), (2, 50), (1, 10), (2, 5), (1, 20), (2, 40)];
    let run = |stable: bool, kind: BinnedRenderPhaseType| {
        let mut world = World::new();
        let view = RetainedViewEntity::new(MainEntity::from(world.spawn_empty().id()), None, 0);
        let entities: Vec<Entity> = queued.iter().map(|_| world.spawn_empty().id()).collect();
        let mut phases = ViewBinnedRenderPhases::<Opaque3d>::default();
        phases.prepare_for_new_frame(view, GpuPreprocessingMode::PreprocessingOnly);
        let batch_set = Opaque3dBatchSetKey { pipeline: CachedRenderPipelineId::new(0), draw_function: DrawFunctionId(0), material_bind_group_index: None, slabs: MeshSlabs::default(), lightmap_slab: None };
        let phase = phases.get_mut(&view).unwrap();
        // Queued last entity first, so neither queue nor entity order is key order.
        for (i, (&(b, _), &e)) in queued.iter().zip(&entities).enumerate().rev() {
            phase.add(batch_set.clone(), bin(b), (e, MainEntity::from(e)), InputUniformIndex(i as u32), kind);
        }
        world.insert_resource(phases);
        if stable {
            let keys = queued.iter().zip(&entities).map(|(&(_, key), &e)| (MainEntity::from(e), key)).collect();
            world.insert_resource(StableBinOrder(keys));
        }
        world.run_system_once(sort_binned_render_phase::<Opaque3d>).unwrap();
        let phases = world.resource::<ViewBinnedRenderPhases<Opaque3d>>();
        let key_of = |m: &MainEntity| queued[entities.iter().position(|e| *e == m.id()).unwrap()].1;
        let phase = &phases[&view];
        match kind {
            BinnedRenderPhaseType::BatchableMesh => phase.batchable_meshes.iter().map(|((_, b), bin)| (b.asset_id, bin.entities().keys().map(key_of).collect::<Vec<_>>())).collect::<Vec<_>>(),
            _ => phase.unbatchable_meshes.iter().map(|((_, b), bin)| (b.asset_id, bin.entities.keys().map(key_of).collect())).collect(),
        }
    };
    for kind in [BinnedRenderPhaseType::BatchableMesh, BinnedRenderPhaseType::UnbatchableMesh] {
        assert_eq!(run(true, kind), vec![(bin(2).asset_id, vec![5, 40, 50]), (bin(1).asset_id, vec![10, 20, 30])], "{kind:?} with StableBinOrder, key order");
        assert_eq!(run(false, kind), vec![(bin(1).asset_id, vec![20, 10, 30]), (bin(2).asset_id, vec![40, 5, 50])], "{kind:?} without it, Bevy's order");
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
    let (_game, mut app) = engine_with("ready-slow-texture", &path, 3, Some(slow));
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
    let (_game, mut app) = engine_with(name, &path, 30, Some(slow));
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
    let (_game, mut app) = engine_with("ready-method-texture", &dir.join("unused.png"), 3, Some(slow));
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
    let (_game, mut app) = engine_with(name, &path, 30, Some(Slow::shaders(dir.join("assets"), gates.clone())));
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
    let (_game, mut app) = engine_with("ready-never-compiles", &env, 3, Some(slow));
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

/// One quad of a glTF primitive in the plane z = 0: its centre, half size and vertex colour, and
/// whether its material is alpha mask.
struct Quad {
    at: [f32; 2],
    half: f32,
    rgba: [f32; 4],
    mask: bool,
}

/// A glTF of one mesh, a primitive per quad, unlit, as `<name>.gltf` and `<name>.bin`.
fn quads_gltf(dir: &Path, name: &str, quads: &[Quad]) {
    std::fs::create_dir_all(dir).unwrap();
    let mut bin: Vec<u8> = Vec::new();
    let (mut views, mut accessors, mut primitives) = (Vec::new(), Vec::new(), Vec::new());
    let mut view = |bin: &mut Vec<u8>, bytes: Vec<u8>| {
        views.push(json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": bytes.len()}));
        bin.extend(bytes);
        views.len() - 1
    };
    for q in quads {
        let ([x, y], h) = (q.at, q.half);
        let corners = [[x - h, y - h], [x + h, y - h], [x + h, y + h], [x - h, y + h]];
        let floats = |vs: Vec<f32>| vs.into_iter().flat_map(f32::to_le_bytes).collect::<Vec<u8>>();
        let p = view(&mut bin, floats(corners.iter().flat_map(|[a, b]| [*a, *b, 0.0]).collect()));
        let n = view(&mut bin, floats([0.0, 0.0, 1.0].repeat(4)));
        let c = view(&mut bin, floats(q.rgba.repeat(4)));
        let i = view(&mut bin, [0u16, 1, 2, 0, 2, 3].into_iter().flat_map(u16::to_le_bytes).collect());
        let first = accessors.len();
        accessors.push(json!({"bufferView": p, "componentType": 5126, "count": 4, "type": "VEC3", "min": [x - h, y - h, 0.0], "max": [x + h, y + h, 0.0]}));
        accessors.push(json!({"bufferView": n, "componentType": 5126, "count": 4, "type": "VEC3"}));
        accessors.push(json!({"bufferView": c, "componentType": 5126, "count": 4, "type": "VEC4"}));
        accessors.push(json!({"bufferView": i, "componentType": 5123, "count": 6, "type": "SCALAR"}));
        primitives.push(json!({"attributes": {"POSITION": first, "NORMAL": first + 1, "COLOR_0": first + 2}, "indices": first + 3, "material": usize::from(q.mask)}));
    }
    std::fs::write(dir.join(format!("{name}.bin")), &bin).unwrap();
    let unlit = json!({"KHR_materials_unlit": {}});
    let gltf = json!({
        "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0}],
        "extensionsUsed": ["KHR_materials_unlit"],
        "materials": [{"extensions": unlit}, {"alphaMode": "MASK", "alphaCutoff": 0.5, "extensions": unlit}],
        "meshes": [{"primitives": primitives}],
        "buffers": [{"uri": format!("{name}.bin"), "byteLength": bin.len()}],
        "bufferViews": views, "accessors": accessors,
    });
    std::fs::write(dir.join(format!("{name}.gltf")), gltf.to_string()).unwrap();
}

/// Two quads in one plane, one red and one blue, at the same place: every pixel is at one depth in
/// both, so the one drawn last decides its colour. Entities spawn red first; `blue_first` adds the
/// blue mesh asset first, so the mesh `AssetId`s, and the bin order they key, flip.
fn mesh_tie_shot(name: &str, blue_first: bool) -> Vec<u8> {
    let dir = shots_dir(name);
    let path = dir.join("tie.png");
    let (_game, mut app) = engine(name, &path, 30);
    app.insert_resource(FixedStep).add_systems(Startup, move |mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>| {
        let material = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() });
        let quad = |c: [f32; 4]| Mesh::from(Rectangle::new(2.5, 2.5)).with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![c; 4]);
        let [red, blue] = [[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]].map(quad);
        let (red, blue) = if blue_first {
            let blue = meshes.add(blue);
            (meshes.add(red), blue)
        } else {
            (meshes.add(red), meshes.add(blue))
        };
        commands.spawn((Mesh3d(red), MeshMaterial3d(material.clone())));
        commands.spawn((Mesh3d(blue), MeshMaterial3d(material)));
    });
    start(&mut app);
    apply_script(&mut app);
    until_shot(&mut app, &path);
    png(&path).data.expect("the shot's pixels")
}

/// Two entities, each a mesh, a material and a transform.
type Pair = [(Handle<Mesh>, Handle<StandardMaterial>, Transform); 2];

/// Shoots `scene`'s two entities under the fixed step, spawned in its order, or the other way round
/// with `flip`: their entity ids, and so their `MainEntity` order, flip with it.
fn spawn_order_shot(name: &str, flip: bool, scene: fn(&mut Assets<Mesh>, &mut Assets<StandardMaterial>) -> Pair) -> Vec<u8> {
    let dir = shots_dir(name);
    let path = dir.join("tie.png");
    let (_game, mut app) = engine(name, &path, 30);
    app.insert_resource(FixedStep).add_systems(Startup, move |mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>| {
        let mut pair = scene(&mut meshes, &mut materials);
        if flip {
            pair.reverse();
        }
        for (mesh, material, transform) in pair {
            commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), transform));
        }
    });
    start(&mut app);
    apply_script(&mut app);
    until_shot(&mut app, &path);
    png(&path).data.expect("the shot's pixels")
}

/// A slab whose front is red and back blue, twice at one place, the second turned exactly half a
/// turn about y: the same bounds, and the second's blue back exactly on the first's red front.
fn half_turn(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Pair {
    let mut slab = Mesh::from(Cuboid::new(2.5, 2.5, 0.5));
    let normals = slab.attribute(Mesh::ATTRIBUTE_NORMAL).and_then(|a| a.as_float3()).expect("a cuboid's normals").to_vec();
    let colours: Vec<[f32; 4]> = normals.iter().map(|n| if n[2] > 0.5 { [1.0, 0.0, 0.0, 1.0] } else if n[2] < -0.5 { [0.0, 0.0, 1.0, 1.0] } else { [0.5, 0.5, 0.5, 1.0] }).collect();
    slab.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    let mesh = meshes.add(slab);
    let material = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() });
    let turned = Transform::from_rotation(Quat::from_xyzw(0.0, 1.0, 0.0, 0.0));
    [(mesh.clone(), material.clone(), Transform::IDENTITY), (mesh, material, turned)]
}

/// One quad, twice at one place, in a red material and a blue one.
fn two_materials(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Pair {
    let mesh = meshes.add(Rectangle::new(2.5, 2.5));
    let [red, blue] = [Color::srgb(1.0, 0.0, 0.0), Color::srgb(0.0, 0.0, 1.0)].map(|c| materials.add(StandardMaterial { base_color: c, unlit: true, ..default() }));
    [(mesh.clone(), red, Transform::IDENTITY), (mesh, blue, Transform::IDENTITY)]
}

#[test]
fn one_mesh_turned_half_a_turn_follows_no_spawn_order() {
    let (a, b) = (spawn_order_shot("half-turn-a", false, half_turn), spawn_order_shot("half-turn-b", true, half_turn));
    assert!(a == b, "the spawn order decided the tie: {} of {} bytes differ", differ(&a, &b), a.len());
}

#[test]
fn two_materials_on_one_mesh_follow_no_spawn_order() {
    let (a, b) = (spawn_order_shot("two-materials-a", false, two_materials), spawn_order_shot("two-materials-b", true, two_materials));
    assert!(a == b, "the spawn order decided the tie: {} of {} bytes differ", differ(&a, &b), a.len());
}

#[derive(Resource)]
struct Scenes(Vec<Handle<bevy::world_serialization::WorldAsset>>);

/// Shoots the glTFs `g0.gltf`.. that `assets` writes into the folder it is given, read from a slow disk and spawned in that order at
/// `places`. With `first`, its read is released first and the others once its scene has loaded;
/// with `None` all go at once, the order left to the IO threads. `setup` adds the rest of the scene.
fn gltf_shot(name: &str, assets: impl FnOnce(&Path), places: &[Vec3], first: Option<usize>, work: Duration, setup: impl FnOnce(&mut App)) -> Vec<u8> {
    gltf_shot_dump(name, assets, places, first, work, setup).0
}

/// As [`gltf_shot`], with the shot's order dump when `setup` asked for one.
fn gltf_shot_dump(name: &str, assets: impl FnOnce(&Path), places: &[Vec3], first: Option<usize>, work: Duration, setup: impl FnOnce(&mut App)) -> (Vec<u8>, Option<String>) {
    let dir = shots_dir(name);
    let path = dir.join("shot.png");
    let folder = dir.join("assets");
    assets(&folder);
    let count = places.len();
    let gates = Gates::held(count);
    match first {
        Some(n) => gates.release(n),
        None => (0..count).for_each(|n| gates.release(n)),
    }
    let (_game, mut app) = engine_with(name, &path, 30, Some(Slow::shaders(folder, gates.clone())));
    let places = places.to_vec();
    app.insert_resource(FixedStep)
        .add_systems(Startup, move |mut commands: Commands, server: Res<AssetServer>| {
            let scenes: Vec<_> = (0..places.len()).map(|n| server.load(format!("slow://g{n}.gltf#Scene0"))).collect();
            for (scene, at) in scenes.iter().zip(&places) {
                commands.spawn((WorldAssetRoot(scene.clone()), Transform::from_translation(*at)));
            }
            commands.insert_resource(Scenes(scenes));
        })
        .add_systems(Update, move || std::thread::sleep(work));
    setup(&mut app);
    start(&mut app);
    apply_script(&mut app);
    let mut released = first.is_none();
    until(&mut app, &format!("{} written", path.display()), |app| {
        if !released {
            let server = app.world().resource::<AssetServer>();
            let first = first.unwrap();
            if app.world().get_resource::<Scenes>().is_some_and(|s| server.is_loaded_with_dependencies(&s.0[first])) {
                (0..count).for_each(|n| gates.release(n));
                released = true;
            }
        }
        std::fs::metadata(&path).is_ok_and(|m| m.len() > 0) && !app.world().contains_resource::<ShotWhenReady>()
    });
    for _ in 0..5 {
        app.update();
    }
    (png(&path).data.expect("the shot's pixels"), std::fs::read_to_string(OrderDump::path(&path)).ok())
}

/// The red and blue quads of [`mesh_tie_shot`] as two glTFs, `g0` red and `g1` blue.
fn tie_gltfs(assets: &Path) {
    quads_gltf(assets, "g0", &[Quad { at: [0.0, 0.0], half: 1.25, rgba: [1.0, 0.0, 0.0, 1.0], mask: false }]);
    quads_gltf(assets, "g1", &[Quad { at: [0.0, 0.0], half: 1.25, rgba: [0.0, 0.0, 1.0, 1.0], mask: false }]);
}

fn differ(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

#[test]
fn a_tie_at_one_depth_follows_no_load_order() {
    let red = mesh_tie_shot("tie-red-mesh-first", false);
    let blue = mesh_tie_shot("tie-blue-mesh-first", true);
    assert!(red == blue, "the mesh asset order decided the tie: {} of {} bytes differ", differ(&red, &blue), red.len());
    let at = [Vec3::ZERO, Vec3::ZERO];
    let red = gltf_shot("tie-red-gltf-first", tie_gltfs, &at, Some(0), Duration::ZERO, |_| {});
    let blue = gltf_shot("tie-blue-gltf-first", tie_gltfs, &at, Some(1), Duration::ZERO, |_| {});
    assert!(red == blue, "the glTF load order decided the tie: {} of {} bytes differ", differ(&red, &blue), red.len());
}

/// An order dump's lines without their entity ids, which follow load timing (after a tab).
fn without_ids(dump: &str) -> String {
    dump.lines().map(|l| l.split('\t').next().unwrap_or_default()).collect::<Vec<_>>().join("\n")
}

#[test]
fn an_order_dump_is_the_same_every_run_and_only_when_asked() {
    let at = [Vec3::ZERO, Vec3::ZERO];
    let asked = |app: &mut App| {
        app.insert_resource(OrderDump::default());
    };
    let dumps: Vec<String> = (0..2).map(|i| gltf_shot_dump(&format!("order-dump-{i}"), tie_gltfs, &at, None, Duration::ZERO, asked).1.expect("a dump beside the shot")).collect();
    for line in ["camera order=0 transform=", "view order=0 transform=", "  Opaque3d bins=2 ", "entity key=", "material=bevy_pbr::"] {
        assert!(dumps[0].contains(line), "no `{line}` in the dump:\n{}", dumps[0]);
    }
    assert!(!without_ids(&dumps[0]).contains(" - ") && !dumps[0].contains("* "), "an item with no key, or a shared one:\n{}", dumps[0]);
    assert_eq!(without_ids(&dumps[0]), without_ids(&dumps[1]), "two runs' dumps differ");
    let (_, none) = gltf_shot_dump("order-dump-off", tie_gltfs, &at, None, Duration::ZERO, |_| {});
    assert!(none.is_none(), "a dump with no OrderDump");
}

#[test]
fn a_gltf_race_gives_one_shot() {
    let at = [Vec3::ZERO, Vec3::ZERO];
    let shots: Vec<Vec<u8>> = (0..10).map(|i| gltf_shot(&format!("tie-race-{i}"), tie_gltfs, &at, None, Duration::ZERO, |_| {})).collect();
    for (i, shot) in shots.iter().enumerate().skip(1) {
        assert!(shots[0] == *shot, "race {i} differs from race 0 in {} of {} bytes", differ(&shots[0], shot), shot.len());
    }
}

/// Agent town's civ forest in small: three tree glTFs, each an opaque trunk and an alpha-mask
/// foliage primitive in its own colour, every foliage redrawn in one shared leaf material, the
/// foliage overlapping in one plane; and two half-transparent sprites at one z, drawn over it by a
/// 2D camera.
fn town_assets(assets: &Path) {
    for (n, rgba) in [[0.1, 0.6, 0.1, 1.0], [0.2, 0.8, 0.3, 1.0], [0.5, 0.7, 0.1, 1.0]].into_iter().enumerate() {
        let trunk = Quad { at: [0.0, -0.9], half: 0.15, rgba: [0.4, 0.25, 0.1, 1.0], mask: false };
        quads_gltf(assets, &format!("g{n}"), &[trunk, Quad { at: [0.0, 0.2], half: 0.8, rgba, mask: true }]);
    }
    for (n, rgba) in [[200, 40, 40, 255], [40, 40, 200, 255]].into_iter().enumerate() {
        std::fs::write(assets.join(format!("s{n}.png")), png_of(rgba)).unwrap();
    }
}

fn town_shot(name: &str, work: Duration) -> Vec<u8> {
    let places = [Vec3::new(-0.6, 0.0, 0.0), Vec3::ZERO, Vec3::new(0.6, 0.0, 0.0)];
    gltf_shot(name, town_assets, &places, None, work, |app| {
        app.add_systems(Startup, |mut commands: Commands, server: Res<AssetServer>| {
            commands.spawn((Camera2d, Camera { order: 1, clear_color: ClearColorConfig::None, ..default() }));
            for (n, x) in [-60.0, 60.0].into_iter().enumerate() {
                let sprite = Sprite { image: server.load(format!("slow://s{n}.png")), color: Color::srgba(1.0, 1.0, 1.0, 0.5), custom_size: Some(Vec2::splat(200.0)), ..default() };
                commands.spawn((sprite, Transform::from_xyz(x, -150.0, 0.0)));
            }
        })
        .add_systems(Update, |mut commands: Commands, added: Query<(Entity, &MeshMaterial3d<StandardMaterial>), Added<MeshMaterial3d<StandardMaterial>>>, mut materials: ResMut<Assets<StandardMaterial>>, mut leaf: Local<Option<Handle<StandardMaterial>>>| {
            for (e, material) in &added {
                if materials.get(&material.0).is_some_and(|m| matches!(m.alpha_mode, AlphaMode::Mask(_))) {
                    let leaf = leaf.get_or_insert_with(|| materials.add(StandardMaterial { base_color: Color::WHITE, alpha_mode: AlphaMode::Mask(0.5), unlit: true, ..default() }));
                    commands.entity(e).insert(MeshMaterial3d(leaf.clone()));
                }
            }
        });
    })
}

#[test]
fn a_town_like_scene_is_the_same_every_shot() {
    let shots: Vec<Vec<u8>> = (0..10).map(|i| town_shot(&format!("town-{i}"), Duration::from_millis(if i % 2 == 1 { 30 } else { 0 }))).collect();
    let colours = shots[0].chunks(4).map(|p| [p[0], p[1], p[2]]).collect::<HashSet<_>>().len();
    assert!(colours > 6, "the trees and sprites are in the shot: {colours} colours");
    for (i, shot) in shots.iter().enumerate().skip(1) {
        assert!(shots[0] == *shot, "shot {i} differs from shot 0 in {} of {} bytes", differ(&shots[0], shot), shot.len());
    }
}
