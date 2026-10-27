//! 10,000 sprites, each with a child (a shadow), in a `MinimalPlugins` app with Bevy's transform
//! propagation and `Pixel2dPlugin`: no renderer, since the pass and propagation are what it adds.
//! Arms: `off` (no pixel-2D component, the pass skipped), `ysort-still` (every sprite y-sorted,
//! none moving), `ysort-moving` (every sprite y-sorted and moving each frame), `all-moving`
//! (`ysort-moving`, a tenth with `ScreenSize`, under a moving `PixelCamera` easing between zoom 2
//! and 4). Each sprite and shadow holds a `Mesh2d`, as a drawn sprite does, so the re-queue of a
//! moved z (`requeue_new_z`) does its work. Prints one line: the pass, propagation, the camera snap
//! and the re-queue per frame, median and p95, over 600 frames after 120 warm-up.

use bevy::camera::{CameraUpdateSystems, RenderTargetInfo};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_ts_material::pixel2d::{requeue_new_z, snap_pixel_camera};
use bevy_ts_material::{PixelCamera, Pixel2dPlugin, Pixel2dSystems, ScreenSize, YSort};
use std::time::Instant;

const SPRITES: usize = 10_000;
const WARMUP: usize = 120;
const FRAMES: usize = 600;

#[derive(Resource, Default)]
struct Marks {
    start: Option<Instant>,
    pass: Vec<f64>,
    propagate: Vec<f64>,
    snap: Vec<f64>,
    requeue: Vec<f64>,
    snapped: Option<Instant>,
    mid: Option<Instant>,
    end: Option<Instant>,
}

#[derive(Resource)]
struct Moving(bool);

#[derive(Component)]
struct Walker(f32);

fn walk(moving: Res<Moving>, time: Res<Time>, mut q: Query<(&Walker, &mut Transform)>) {
    if !moving.0 {
        return;
    }
    let t = time.elapsed_secs();
    for (w, mut tf) in &mut q {
        tf.translation.y = (t + w.0).sin() * 200.0;
    }
}

/// The pixel camera between zoom 2 and 4 every half second, easing, and moving, so every
/// `ScreenSize` changes and the snap shifts most frames.
fn zoom(time: Res<Time>, mut cams: Query<(&mut PixelCamera, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (mut p, mut tf) in &mut cams {
        p.zoom = if (t * 2.0) as u32 % 2 == 0 { 2.0 } else { 4.0 };
        tf.translation.x = t * 37.3;
    }
}

fn start(mut m: ResMut<Marks>) {
    m.start = Some(Instant::now());
}

fn mid(mut m: ResMut<Marks>) {
    let now = Instant::now();
    let s = m.start.take().unwrap();
    m.pass.push((now - s).as_secs_f64() * 1e3);
    m.mid = Some(now);
}

fn end(mut m: ResMut<Marks>) {
    let s = m.mid.take().unwrap();
    let now = Instant::now();
    m.propagate.push((now - s).as_secs_f64() * 1e3);
    m.end = Some(now);
}

fn snapped(mut m: ResMut<Marks>) {
    let s = m.end.take().unwrap();
    let now = Instant::now();
    m.snap.push((now - s).as_secs_f64() * 1e3);
    m.snapped = Some(now);
}

fn requeued(mut m: ResMut<Marks>) {
    let s = m.snapped.take().unwrap();
    m.requeue.push(s.elapsed().as_secs_f64() * 1e3);
}

fn main() {
    let arm = std::env::args().nth(1).unwrap_or_else(|| "ysort-moving".into());
    let (sorted, moving, all) = match arm.as_str() {
        "off" => (false, true, false),
        "ysort-still" => (true, false, false),
        "ysort-moving" => (true, true, false),
        "all-moving" => (true, true, true),
        a => panic!("arm {a:?}: off, ysort-still, ysort-moving or all-moving"),
    };
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::transform::TransformPlugin, Pixel2dPlugin))
        .init_resource::<Marks>()
        .insert_resource(Moving(moving))
        .add_systems(Update, (walk, zoom.run_if(move || all)))
        .add_systems(
            PostUpdate,
            (
                start.before(Pixel2dSystems),
                mid.after(Pixel2dSystems).before(TransformSystems::Propagate),
                end.after(TransformSystems::Propagate).before(snap_pixel_camera),
                snapped.after(snap_pixel_camera).after(CameraUpdateSystems).before(requeue_new_z),
                requeued.after(requeue_new_z),
            ),
        );
    let world = app.world_mut();
    for i in 0..SPRITES {
        let (x, y) = ((i % 100) as f32 * 16.0, (i / 100) as f32 * 16.0);
        let mut e = world.spawn((Transform::from_xyz(x, y, 0.0), Mesh2d::default(), Walker(i as f32 * 0.01)));
        if sorted {
            e.insert(YSort { layer: 1.0, offset: -8.0 });
        }
        if all && i % 10 == 0 {
            e.insert(ScreenSize::default());
        }
        e.with_child((Transform::from_xyz(0.0, -8.0, -1e-5), Mesh2d::default()));
    }
    if all {
        let mut camera = Camera::default();
        camera.computed.target_info = Some(RenderTargetInfo { physical_size: UVec2::new(1280, 720), scale_factor: 1.0 });
        world.spawn((Camera2d, camera, PixelCamera { zoom: 2.0, ease: 0.25 }));
    }
    for _ in 0..WARMUP {
        app.update();
    }
    {
        let mut m = app.world_mut().resource_mut::<Marks>();
        m.pass.clear();
        m.propagate.clear();
        m.snap.clear();
        m.requeue.clear();
    }
    for _ in 0..FRAMES {
        app.update();
    }
    let m = app.world_mut().resource_mut::<Marks>();
    let at = |v: &[f64], p: usize| {
        let mut v = v.to_vec();
        v.sort_by(f64::total_cmp);
        v[v.len() * p / 100]
    };
    println!(
        "arm={arm} sprites={SPRITES} pass_median_ms={:.3} pass_p95_ms={:.3} propagate_median_ms={:.3} propagate_p95_ms={:.3} snap_median_ms={:.3} snap_p95_ms={:.3} requeue_median_ms={:.3} requeue_p95_ms={:.3}",
        at(&m.pass, 50),
        at(&m.pass, 95),
        at(&m.propagate, 50),
        at(&m.propagate, 95),
        at(&m.snap, 50),
        at(&m.snap, 95),
        at(&m.requeue, 50),
        at(&m.requeue, 95),
    );
}
