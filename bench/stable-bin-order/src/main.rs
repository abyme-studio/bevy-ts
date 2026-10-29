//! A civ-like frame under the fixed step: `TREES` trees from three glTFs, each an opaque trunk and
//! an alpha-mask foliage primitive redrawn in one shared leaf material, their foliage overlapping,
//! and `SPRITES` half-transparent sprites at one z over them. Headless, shot at the fixed step's
//! 60th ready frame. Prints the wall time to the shot and the render world's `PhaseSort` set per
//! frame (where `sort_binned_render_phase` and `sort_phase_system` run), mean and p95.

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::world_serialization::WorldAssetRoot;
use bevy_ts_remote::screenshot::{FixedStep, ReadyPlugin, ScreenshotTarget, ShotWhenReady};
use serde_json::json;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TREES: usize = 1500;
const SPRITES: usize = 500;

#[derive(Clone, Default)]
struct Sorts(Arc<Mutex<(Option<Instant>, Vec<Duration>)>>);

fn main() {
    let dir = std::env::temp_dir().join(format!("stable-bin-order-bench-{}", std::process::id()));
    let assets = dir.join("assets");
    write_assets(&assets);
    let shot = dir.join("shot.png");

    let started = Instant::now();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(AssetPlugin { file_path: assets.to_string_lossy().into_owned(), ..default() }),
    );
    let mut image = Image::new_target_texture(960, 540, TextureFormat::Rgba8UnormSrgb, None);
    image.asset_usage = RenderAssetUsages::default();
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    app.insert_resource(ScreenshotTarget(RenderTarget::Image(target.clone().into())))
        .insert_resource(FixedStep)
        .insert_resource(ShotWhenReady::new(&shot))
        .insert_resource(ClearColor(Color::BLACK))
        .add_plugins(ReadyPlugin)
        .add_systems(Startup, move |commands: Commands, server: Res<AssetServer>| scene(commands, server, target.clone()))
        .add_systems(Update, shared_leaf);
    let sorts = Sorts::default();
    let (before, after) = (sorts.clone(), sorts.clone());
    // As `App::run`'s runner: the renderer is made asynchronously, and `RenderApp` exists only
    // once the plugins are ready.
    while app.plugins_state() == bevy::app::PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    // Before `cleanup`, which moves `RenderApp` to the render thread (`PipelinedRenderingPlugin`).
    app.sub_app_mut(RenderApp).add_systems(
        Render,
        (
            (move || before.0.lock().unwrap().0 = Some(Instant::now())).after(RenderSystems::QueueSweep).before(RenderSystems::PhaseSort),
            (move || {
                let mut s = after.0.lock().unwrap();
                if let Some(t) = s.0.take() {
                    s.1.push(t.elapsed());
                }
            })
            .after(RenderSystems::PhaseSort)
            .before(RenderSystems::Prepare),
        ),
    );
    app.cleanup();
    while !(std::fs::metadata(&shot).is_ok_and(|m| m.len() > 0) && !app.world().contains_resource::<ShotWhenReady>()) {
        app.update();
        assert!(started.elapsed() < Duration::from_secs(600), "no shot in 10 min");
    }
    let shot_time = started.elapsed();
    let mut times = sorts.0.lock().unwrap().1.clone();
    times.sort();
    let mean = times.iter().sum::<Duration>() / times.len().max(1) as u32;
    let p95 = times.get(times.len() * 95 / 100).copied().unwrap_or_default();
    println!(
        "shot_ms={} phase_sort_mean_us={} phase_sort_p95_us={} frames={}",
        shot_time.as_millis(),
        mean.as_micros(),
        p95.as_micros(),
        times.len()
    );
    drop(app);
    let _ = std::fs::remove_dir_all(&dir);
}

fn scene(mut commands: Commands, server: Res<AssetServer>, target: Handle<Image>) {
    let target = RenderTarget::Image(target.into());
    commands.spawn((Camera3d::default(), target.clone(), Transform::from_xyz(0.0, 30.0, 40.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((DirectionalLight::default(), Transform::from_xyz(1.0, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((Camera2d, Camera { order: 1, clear_color: ClearColorConfig::None, ..default() }, target));
    let trees: Vec<Handle<bevy::world_serialization::WorldAsset>> = (0..3).map(|n| server.load(format!("g{n}.gltf#Scene0"))).collect();
    let side = (TREES as f32).sqrt().ceil() as usize;
    for i in 0..TREES {
        let (x, z) = ((i % side) as f32 - side as f32 / 2.0, (i / side) as f32 - side as f32 / 2.0);
        commands.spawn((WorldAssetRoot(trees[i % 3].clone()), Transform::from_xyz(x * 1.2, 0.0, z * 1.2)));
    }
    let images: Vec<Handle<Image>> = (0..2).map(|n| server.load(format!("s{n}.png"))).collect();
    for i in 0..SPRITES {
        let (x, y) = ((i % 25) as f32 * 30.0 - 360.0, (i / 25) as f32 * 20.0 - 200.0);
        let sprite = Sprite { image: images[i % 2].clone(), color: Color::srgba(1.0, 1.0, 1.0, 0.5), custom_size: Some(Vec2::splat(48.0)), ..default() };
        commands.spawn((sprite, Transform::from_xyz(x, y, 0.0)));
    }
}

/// Every alpha-mask material a glTF brings, redrawn in one shared leaf material, as agent town does.
fn shared_leaf(
    mut commands: Commands,
    added: Query<(Entity, &MeshMaterial3d<StandardMaterial>), Added<MeshMaterial3d<StandardMaterial>>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut leaf: Local<Option<Handle<StandardMaterial>>>,
) {
    for (e, material) in &added {
        if materials.get(&material.0).is_some_and(|m| matches!(m.alpha_mode, AlphaMode::Mask(_))) {
            let leaf = leaf.get_or_insert_with(|| materials.add(StandardMaterial { base_color: Color::WHITE, alpha_mode: AlphaMode::Mask(0.5), unlit: true, ..default() }));
            commands.entity(e).insert(MeshMaterial3d(leaf.clone()));
        }
    }
}

/// Three tree glTFs (`g0`..`g2`): a trunk quad and a foliage quad in the plane z = 0, the foliage
/// alpha mask in its own vertex colour; and two sprite images.
fn write_assets(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    for (n, rgba) in [[0.1f32, 0.6, 0.1, 1.0], [0.2, 0.8, 0.3, 1.0], [0.5, 0.7, 0.1, 1.0]].into_iter().enumerate() {
        let quads = [([0.0f32, 0.4], 0.15f32, [0.4f32, 0.25, 0.1, 1.0], false), ([0.0, 1.4], 0.8, rgba, true)];
        let mut bin: Vec<u8> = Vec::new();
        let (mut views, mut accessors, mut primitives) = (Vec::new(), Vec::new(), Vec::new());
        let mut view = |bin: &mut Vec<u8>, bytes: Vec<u8>| {
            views.push(json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": bytes.len()}));
            bin.extend(bytes);
            views.len() - 1
        };
        let floats = |vs: Vec<f32>| vs.into_iter().flat_map(f32::to_le_bytes).collect::<Vec<u8>>();
        for ([x, y], h, rgba, mask) in quads {
            let corners = [[x - h, y - h], [x + h, y - h], [x + h, y + h], [x - h, y + h]];
            let p = view(&mut bin, floats(corners.iter().flat_map(|[a, b]| [*a, *b, 0.0]).collect()));
            let nv = view(&mut bin, floats([0.0, 0.0, 1.0].repeat(4)));
            let c = view(&mut bin, floats(rgba.repeat(4)));
            let i = view(&mut bin, [0u16, 1, 2, 0, 2, 3].into_iter().flat_map(u16::to_le_bytes).collect());
            let first = accessors.len();
            accessors.push(json!({"bufferView": p, "componentType": 5126, "count": 4, "type": "VEC3", "min": [x - h, y - h, 0.0], "max": [x + h, y + h, 0.0]}));
            accessors.push(json!({"bufferView": nv, "componentType": 5126, "count": 4, "type": "VEC3"}));
            accessors.push(json!({"bufferView": c, "componentType": 5126, "count": 4, "type": "VEC4"}));
            accessors.push(json!({"bufferView": i, "componentType": 5123, "count": 6, "type": "SCALAR"}));
            primitives.push(json!({"attributes": {"POSITION": first, "NORMAL": first + 1, "COLOR_0": first + 2}, "indices": first + 3, "material": usize::from(mask)}));
        }
        std::fs::write(dir.join(format!("g{n}.bin")), &bin).unwrap();
        let unlit = json!({"KHR_materials_unlit": {}});
        let gltf = json!({
            "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0}],
            "extensionsUsed": ["KHR_materials_unlit"],
            "materials": [{"extensions": unlit}, {"alphaMode": "MASK", "alphaCutoff": 0.5, "extensions": unlit}],
            "meshes": [{"primitives": primitives}],
            "buffers": [{"uri": format!("g{n}.bin"), "byteLength": bin.len()}],
            "bufferViews": views, "accessors": accessors,
        });
        std::fs::write(dir.join(format!("g{n}.gltf")), gltf.to_string()).unwrap();
    }
    for (n, rgba) in [[200u8, 40, 40, 255], [40, 40, 200, 255]].into_iter().enumerate() {
        let image = Image::new(
            bevy::render::render_resource::Extent3d { width: 4, height: 4, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            rgba.repeat(16),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.try_into_dynamic().unwrap().save(dir.join(format!("s{n}.png"))).unwrap();
    }
}
