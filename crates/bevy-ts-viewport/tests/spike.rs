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

/// Flips the clear colour between two values on each `KeyD` press.
fn flip_on_key(keys: Res<ButtonInput<KeyCode>>, mut cameras: Query<&mut Camera>, mut on: Local<bool>) {
    if keys.just_pressed(KeyCode::KeyD) {
        *on = !*on;
        let c = if *on { Color::srgb(1.0, 0.0, 0.0) } else { Color::srgb(0.0, 0.0, 1.0) };
        for mut cam in &mut cameras {
            cam.clear_color = ClearColorConfig::Custom(c);
        }
    }
}

/// Task 1.5: from a key leaving the viewer to the announced frame that shows it, over 200 presses
/// with updates paced at 120 Hz as the viewer's ticks would pace them. `SPIKE_PRESSES` overrides.
#[test]
#[ignore = "timing: needs a release build and a quiet machine; the viewport-latency process runs it"]
fn key_to_pixels() {
    use bevy_ts_viewport::shared::now_ns;
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() }),
    )
    .add_plugins(ViewportPlugin::default())
    .add_systems(Startup, scene)
    .add_systems(Update, flip_on_key);
    app.finish();
    app.cleanup();
    let name = attach(app.world_mut()).expect("attach");
    let viewer = Viewer::connect(&name).expect("connect");
    viewer.send("input", serde_json::json!({ "type": "focus", "on": true })).unwrap();
    let mut surfaces: HashMap<(u64, u64), Surface> = HashMap::new();
    let period = Duration::from_secs_f64(1.0 / 120.0);
    // Returns the newest frame's (ts, top-left pixel).
    let pump = |surfaces: &mut HashMap<(u64, u64), Surface>| -> Option<(u64, [u8; 4])> {
        let mut newest = None;
        while let Some(m) = viewer.recv(Duration::ZERO) {
            let Incoming::Message { method, params, port } = m else { continue };
            let key = (params["ring"].as_u64().unwrap_or(0), params["slot"].as_u64().unwrap_or(0));
            match method.as_str() {
                "surface" => {
                    let port = port.unwrap();
                    surfaces.insert(key, Surface::from_mach_port(port).unwrap());
                    release_port(port);
                }
                "frame" => newest = Some((params["ts"].as_u64().unwrap(), surfaces[&key].read(&[(0, 0)])[0])),
                _ => {}
            }
        }
        newest
    };
    let tick = |app: &mut App| {
        let t0 = Instant::now();
        app.update();
        if let Some(rest) = period.checked_sub(t0.elapsed()) {
            std::thread::sleep(rest);
        }
    };
    let warm = Instant::now();
    let mut last = None;
    while last.is_none() && warm.elapsed() < Duration::from_secs(60) {
        tick(&mut app);
        last = pump(&mut surfaces).map(|f| f.1);
    }
    for _ in 0..30 {
        tick(&mut app);
        if let Some(f) = pump(&mut surfaces) {
            last = Some(f.1);
        }
    }
    let presses: usize = std::env::var("SPIKE_PRESSES").ok().and_then(|s| s.parse().ok()).unwrap_or(20);
    let mut samples = vec![];
    for i in 0..presses {
        let before = last.unwrap();
        // Press at a random phase of the frame, as a person would.
        std::thread::sleep(Duration::from_micros((i as u64 * 1777) % 8333));
        let sent = now_ns();
        viewer.send("input", serde_json::json!({ "type": "key", "state": "down", "code": "KeyD" })).unwrap();
        let t0 = Instant::now();
        loop {
            tick(&mut app);
            if let Some((ts, px)) = pump(&mut surfaces) {
                last = Some(px);
                if px != before {
                    samples.push((ts - sent) as f64 / 1e6);
                    break;
                }
            }
            assert!(t0.elapsed() < Duration::from_secs(2), "press {i} never showed");
        }
        viewer.send("input", serde_json::json!({ "type": "key", "state": "up", "code": "KeyD" })).unwrap();
        for _ in 0..3 {
            tick(&mut app);
            if let Some(f) = pump(&mut surfaces) {
                last = Some(f.1);
            }
        }
    }
    samples.sort_by(f64::total_cmp);
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    let shared = app.world().resource::<bevy_ts_viewport::shared::SharedRes>().0.clone();
    eprintln!(
        "key to pixels over {} presses at 120 Hz: median {:.2} ms, p95 {:.2} ms, max {:.2} ms; engine stats {}; gpu {:?} ms",
        samples.len(),
        at(0.5),
        at(0.95),
        samples.last().unwrap(),
        shared.stats.latency(),
        shared.stats.gpu_ms()
    );
    assert!(at(0.5) <= 16.7, "median key to pixels {:.2} ms is over two refreshes at 120 Hz", at(0.5));
}
