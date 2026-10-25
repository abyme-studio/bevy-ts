//! 1,000 world-text labels from Rust (distinct names, two colours, one outline style) under a
//! `Camera3d` at 1280×720 offscreen, the engine's renderer, pipelining off so a frame is one
//! `update`. Arms: `empty` (the scene, no labels), `still` (1,000 labels, nothing changes),
//! `churn` (1,000 labels, 100 of them given new text each frame). Prints one line: first mount
//! (from spawn until every label has its mesh, atlas growth included), frame time median and p95
//! over 600 frames after 240 warm-up, the median with each frame waited out on the GPU, labels
//! laid out per frame, and the material and atlas sizes.

use bevy::app::ScheduleRunnerPlugin;
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::render_resource::TextureFormat;
use bevy::render::renderer::RenderDevice;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts_material::world_text::{WorldTextAtlas, WorldTextMaterials};
use bevy_ts_material::{LaidOut, WorldText, WorldTextPlugin, WorldTextStyle};
use std::time::{Duration, Instant};

const SIZE: (u32, u32) = (1280, 720);
const LABELS: usize = 1000;
const CHURN: usize = 100;
const WARMUP: usize = 240;
const FRAMES: usize = 600;

const SYLLABLES: [&str; 16] = ["ka", "ro", "mi", "ten", "sul", "va", "dor", "en", "li", "ash", "bru", "qui", "zan", "ho", "pel", "yr"];

/// A distinct place name for label `i`.
fn name(i: usize) -> String {
    let s = |k: usize| SYLLABLES[k % SYLLABLES.len()];
    let mut n = format!("{}{}{}", s(i), s(i / 16), s(i / 256));
    n[..1].make_ascii_uppercase();
    format!("{n} {i}")
}

#[derive(Resource, Default)]
struct Churn {
    on: bool,
    frame: usize,
}

/// `churn`: 100 labels, a different hundred each frame, get new text.
fn churn(mut c: ResMut<Churn>, mut q: Query<&mut WorldText>) {
    if !c.on {
        return;
    }
    c.frame += 1;
    let start = (c.frame * CHURN) % LABELS;
    for (i, mut t) in q.iter_mut().enumerate().skip(start).take(CHURN) {
        t.0 = format!("{} {}", name(i), c.frame % 100);
    }
}

fn main() {
    let arm = std::env::args().nth(1).unwrap_or_else(|| "still".into());
    let labels = match arm.as_str() {
        "empty" => 0,
        "still" | "churn" => LABELS,
        a => panic!("arm {a:?}: empty, still or churn"),
    };
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<PipelinedRenderingPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::ZERO))
    .add_plugins(WorldTextPlugin)
    .init_resource::<Churn>()
    .add_systems(Update, churn);
    app.finish();
    app.cleanup();
    scene(app.world_mut());
    // The renderer's pipelines and the scene's first frames, before the labels come.
    for _ in 0..60 {
        app.update();
    }

    let t = Instant::now();
    spawn(app.world_mut(), labels);
    let mut frames = 0;
    loop {
        app.update();
        frames += 1;
        let world = app.world_mut();
        let drawn = world.query_filtered::<(), (With<WorldText>, With<Mesh3d>)>().iter(world).count();
        if drawn == labels {
            break;
        }
        assert!(frames < 600, "{drawn} of {labels} labels drawn after {frames} frames");
    }
    let mount_ms = t.elapsed().as_secs_f64() * 1e3;
    app.world_mut().resource_mut::<Churn>().on = arm == "churn";

    for _ in 0..WARMUP {
        app.update();
    }
    let (mut free, mut laid) = (Vec::with_capacity(FRAMES), 0u64);
    for _ in 0..FRAMES {
        let t = Instant::now();
        app.update();
        free.push(t.elapsed().as_secs_f64() * 1e3);
        laid += app.world().resource::<LaidOut>().0 as u64;
    }
    let device = app.world().resource::<RenderDevice>().clone();
    let mut synced = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let t = Instant::now();
        app.update();
        device.wgpu_device().poll(bevy::render::render_resource::PollType::wait_indefinitely()).unwrap();
        synced.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let at = |v: &mut Vec<f64>, p: usize| {
        v.sort_by(f64::total_cmp);
        v[v.len() * p / 100]
    };
    let (median, p95, synced_median) = (at(&mut free, 50), at(&mut free, 95), at(&mut synced, 50));
    let world = app.world();
    let atlas = world.resource::<WorldTextAtlas>();
    println!(
        "arm={arm} labels={labels} mount_ms={mount_ms:.1} mount_frames={frames} frame_median_ms={median:.3} frame_p95_ms={p95:.3} gpu_synced_median_ms={synced_median:.3} laid_out_per_frame={:.1} materials={} atlas={}x{} glyphs={}",
        laid as f64 / FRAMES as f64,
        world.resource::<WorldTextMaterials>().len(),
        atlas.packer.width,
        atlas.packer.height,
        atlas.made,
    );
}

/// A ground, a sun and the camera over it, as a map is seen.
fn scene(world: &mut World) {
    let mut target = Image::new_target_texture(SIZE.0, SIZE.1, TextureFormat::Rgba8UnormSrgb, None);
    target.asset_usage = RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(target);
    world.spawn((Camera3d::default(), RenderTarget::Image(target.into()), Transform::from_xyz(0.0, 30.0, 32.0).looking_at(Vec3::new(0.0, 0.0, 2.0), Vec3::Y)));
    world.spawn((DirectionalLight { shadow_maps_enabled: true, ..default() }, Transform::from_xyz(3.0, 6.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
    let ground = world.resource_mut::<Assets<Mesh>>().add(Plane3d::new(Vec3::Y, Vec2::splat(30.0)));
    let green = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::from(Color::srgb(0.3, 0.5, 0.3)));
    world.spawn((Mesh3d(ground), MeshMaterial3d(green)));
}

/// 1,000 labels on a 40 × 25 grid over the ground, facing the camera's side.
fn spawn(world: &mut World, n: usize) {
    let outline = Some((0.1, Color::BLACK));
    for i in 0..n {
        let (x, z) = ((i % 40) as f32 - 19.5, (i / 40) as f32 - 12.0);
        let color = if i % 2 == 0 { Color::WHITE } else { Color::srgb(1.0, 0.85, 0.3) };
        world.spawn((
            WorldText::new(name(i)),
            WorldTextStyle { size: 0.4, outline, ..default() },
            TextColor(color),
            Transform::from_xyz(x * 1.4, 0.5, z * 2.0).looking_to(Vec3::new(0.0, -0.7, -1.0), Vec3::Y),
        ));
    }
}
