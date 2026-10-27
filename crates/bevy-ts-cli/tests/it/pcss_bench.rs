//! What Bevy's `experimental_pbr_pcss` feature costs the prebuilt engine with soft shadows unset: a
//! civ-like board (a ground and a grid of cubes) under one shadowed sun, drawn offscreen, no
//! window. One arm per build: run it with `--no-default-features`, then as is; each prints its frame
//! times and saves its last frame (kept with BEVY_TS_KEEP_TEST_DIRS=1); PCSS_OTHER_RGBA=<the other
//! arm's kept pcss-*.rgba> makes it compare its frame with that one by pixel.
//! `cargo static test --release -p bevy-ts-cli --test it pcss_bench:: -- --ignored --nocapture`.
#![cfg(target_os = "macos")]

use bevy::camera::RenderTarget;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::DirectionalLight;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use crate::common::TestDir;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const W: u32 = 1280;
const H: u32 = 720;
const WARM: usize = 240;
const FRAMES: usize = 600;

fn arm() -> &'static str {
    if cfg!(feature = "pcss") { "on" } else { "off" }
}

/// The arm's last frame: a PNG to look at, and its raw RGBA to compare.
fn frame_path(dir: &TestDir, arm: &str, ext: &str) -> PathBuf {
    dir.join(format!("pcss-{arm}.{ext}"))
}

#[test]
#[ignore = "a bench: run each arm in release"]
fn pcss_bench() {
    // Held to the end. Each arm is its own process: BEVY_TS_KEEP_TEST_DIRS=1 keeps its frames, and
    // PCSS_OTHER_RGBA names the other arm's kept `.rgba` to compare against.
    let dir = TestDir::new("pcss");
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    );
    while app.plugins_state() == bevy::app::PluginsState::Adding {
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    let world = app.world_mut();
    let mut image = Image::new_target_texture(W, H, TextureFormat::Rgba8UnormSrgb, None);
    image.asset_usage = bevy::asset::RenderAssetUsages::default();
    let target = world.resource_mut::<Assets<Image>>().add(image);
    let ground = world.resource_mut::<Assets<Mesh>>().add(Plane3d::default().mesh().size(40.0, 40.0));
    let cube = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(0.8, 0.8, 0.8));
    let grass = world.resource_mut::<Assets<StandardMaterial>>().add(Color::srgb(0.35, 0.55, 0.3));
    let stone = world.resource_mut::<Assets<StandardMaterial>>().add(Color::srgb(0.8, 0.75, 0.7));
    world.spawn((Mesh3d(ground), MeshMaterial3d(grass)));
    for x in -10i32..10 {
        for z in -10i32..10 {
            let h = 0.4 + ((x * 7 + z * 13).rem_euclid(5)) as f32 * 0.3;
            world.spawn((Mesh3d(cube.clone()), MeshMaterial3d(stone.clone()), Transform::from_xyz(x as f32 * 1.6, h * 0.5, z as f32 * 1.6).with_scale(Vec3::new(1.0, h / 0.8, 1.0))));
        }
    }
    // Soft shadows unset in both arms: the feature's cost when nobody uses it.
    world.spawn((DirectionalLight { illuminance: 10_000.0, shadow_maps_enabled: true, ..default() }, Transform::from_xyz(8.0, 14.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y)));
    world.spawn((Camera3d::default(), Tonemapping::None, Msaa::Off, RenderTarget::Image(target.clone().into()), Transform::from_xyz(0.0, 18.0, 22.0).looking_at(Vec3::ZERO, Vec3::Y)));

    for _ in 0..WARM {
        app.update();
    }
    let mut times = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let t = Instant::now();
        app.update();
        times.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let shot: Arc<Mutex<Option<Image>>> = Arc::default();
    let s = shot.clone();
    app.world_mut().spawn(Screenshot::image(target)).observe(save_to_disk(frame_path(&dir, arm(), "png"))).observe(move |e: On<ScreenshotCaptured>| {
        *s.lock().unwrap() = Some(e.image.clone());
    });
    let img = loop {
        app.update();
        if let Some(i) = shot.lock().unwrap().take() {
            break i;
        }
    };
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!("pcss={} frames={FRAMES} mean_ms={mean:.3} p50_ms={:.3} p95_ms={:.3}", arm(), times[FRAMES / 2], times[FRAMES * 95 / 100]);
    // A frame or two more, so the PNG is written.
    for _ in 0..4 {
        app.update();
    }
    let rgba = img.data.clone().unwrap();
    std::fs::write(frame_path(&dir, arm(), "rgba"), &rgba).unwrap();
    let other = PathBuf::from(std::env::var_os("PCSS_OTHER_RGBA").unwrap_or_default());
    if let Ok(o) = std::fs::read(&other) {
        let (mut differ, mut max) = (0usize, 0u8);
        for (a, b) in rgba.chunks(4).zip(o.chunks(4)) {
            let d = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
            max = max.max(d);
            differ += (d > 0) as usize;
        }
        println!("pcss diff against {}: {differ} of {} pixels differ, max channel diff {max}", other.display(), W * H);
    }
}
