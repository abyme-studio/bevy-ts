//! Task 1 of change `viewport`: a Bevy frame lands in an IOSurface another party reads, through
//! an unmodified `Camera3d` on the virtual primary window. The viewer here is in-process (the
//! Mach channel and IOSurface ports are the same as across processes) and windowless: it reads
//! pixels back instead of showing them.
#![cfg(target_os = "macos")]

use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy_ts_viewport::mach::{Incoming, Viewer, release_port};
use bevy_ts_viewport::surface::Surface;
use bevy_ts_viewport::{ViewportPlugin, attach};
use std::collections::HashMap;
use std::time::{Duration, Instant};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(ScheduleRunnerPlugin::run_once())
    .add_plugins(ViewportPlugin::default())
    .add_systems(Startup, scene)
    .add_systems(Update, pattern);
    app.finish();
    app.cleanup();
    app
}

fn scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((Camera3d::default(), Transform::from_xyz(-3.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((DirectionalLight::default(), Transform::from_xyz(3.0, 6.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y)));
    commands.spawn((Mesh3d(meshes.add(Cuboid::from_size(Vec3::splat(0.5)))), MeshMaterial3d(materials.add(StandardMaterial::default()))));
}

/// A full-screen colour that changes every frame: the clear colour steps through 64 greys.
fn pattern(mut cameras: Query<&mut Camera>, mut n: Local<u32>) {
    *n += 1;
    let v = (*n % 64) as f32 / 63.0;
    for mut c in &mut cameras {
        c.clear_color = ClearColorConfig::Custom(Color::srgb(v, 1.0 - v, 0.5));
    }
}

struct Seen {
    surfaces: HashMap<(u64, u64), Surface>,
    frames: Vec<(u64, u64, u64, u32, u32)>,
    mixed: usize,
}

fn pump(viewer: &Viewer, seen: &mut Seen, wait: Duration) {
    while let Some(m) = viewer.recv(wait) {
        let Incoming::Message { method, params, port } = m else { continue };
        let key = (params["ring"].as_u64().unwrap_or(0), params["slot"].as_u64().unwrap_or(0));
        match method.as_str() {
            "surface" => {
                let port = port.expect("a surface carries its port");
                let s = Surface::from_mach_port(port).expect("IOSurfaceLookupFromMachPort");
                release_port(port);
                seen.surfaces.insert(key, s);
            }
            "frame" => {
                let (w, h) = (params["w"].as_u64().unwrap() as u32, params["h"].as_u64().unwrap() as u32);
                let s = &seen.surfaces[&key];
                // The clear colour fills the corners; the cube is in the middle.
                let px = s.read(&[(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)]);
                if px.iter().any(|p| *p != px[0]) {
                    seen.mixed += 1;
                }
                seen.frames.push((key.0, key.1, params["seq"].as_u64().unwrap(), w, h));
            }
            _ => {}
        }
    }
}

#[test]
fn a_frame_lands_in_the_viewers_surface() {
    let mut app = app();
    let name = attach(app.world_mut()).expect("attach");
    let viewer = Viewer::connect(&name).expect("connect");
    let mut seen = Seen { surfaces: HashMap::new(), frames: vec![], mixed: 0 };
    // Pipelines compile on the first frames; measure after the first frame lands.
    let warm = Instant::now();
    while seen.frames.is_empty() && warm.elapsed() < Duration::from_secs(60) {
        app.update();
        pump(&viewer, &mut seen, Duration::from_millis(5));
    }
    seen.frames.clear();
    let shared = app.world().resource::<bevy_ts_viewport::shared::SharedRes>().0.clone();
    let skipped0 = shared.stats.skipped.load(std::sync::atomic::Ordering::Relaxed);
    let secs: f64 = std::env::var("SPIKE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let started = Instant::now();
    let mut updates = 0u64;
    while started.elapsed().as_secs_f64() < secs {
        let t0 = Instant::now();
        app.update();
        updates += 1;
        pump(&viewer, &mut seen, Duration::ZERO);
        if let Some(rest) = Duration::from_secs_f64(1.0 / 120.0).checked_sub(t0.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    pump(&viewer, &mut seen, Duration::from_millis(100));

    let skipped = shared.stats.skipped.load(std::sync::atomic::Ordering::Relaxed) - skipped0;
    eprintln!(
        "spike: {updates} updates in {:.1}s, {} surfaces, {} frames, {skipped} skipped, {} mixed",
        started.elapsed().as_secs_f64(),
        seen.surfaces.len(),
        seen.frames.len(),
        seen.mixed
    );
    assert_eq!(seen.surfaces.len(), 3, "three surfaces of one ring");
    let s = seen.surfaces.values().next().unwrap();
    assert!(s.width() >= 2560 && s.height() >= 1440, "ring is at least 2560x1440, got {}x{}", s.width(), s.height());
    assert!(seen.frames.len() as u64 > updates / 2, "most updates produce a frame");
    assert!(seen.frames.iter().all(|f| (f.3, f.4) == (2560, 1440)), "frames carry the window's pixel size");
    assert_eq!(seen.mixed, 0, "no frame mixes two patterns");
    // A frame never names a slot twice in a row.
    assert!(seen.frames.windows(2).all(|w| (w[0].0, w[0].1) != (w[1].0, w[1].1)));
}
