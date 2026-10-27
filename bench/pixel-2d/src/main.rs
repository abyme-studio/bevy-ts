//! 10,000 sprites, each with a child (a shadow), in a `MinimalPlugins` app with Bevy's transform
//! propagation and `Pixel2dPlugin`: no renderer, since the pass and propagation are what it adds.
//! Arms: `off` (no pixel-2D component, the pass skipped), `ysort-still` (every sprite y-sorted,
//! none moving), `ysort-moving` (every sprite y-sorted and moving each frame), `all-moving`
//! (`ysort-moving`, a tenth with `ScreenSize`, under a `Camera2d` whose scale changes each frame).
//! Prints one line:
//! the pass and propagation per frame, median and p95, over 600 frames after 120 warm-up.

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_ts_material::{Pixel2dPlugin, Pixel2dSystems, ScreenSize, YSort};
use std::time::Instant;

const SPRITES: usize = 10_000;
const WARMUP: usize = 120;
const FRAMES: usize = 600;

#[derive(Resource, Default)]
struct Marks {
    start: Option<Instant>,
    pass: Vec<f64>,
    propagate: Vec<f64>,
    mid: Option<Instant>,
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

/// The camera's scale between 1/2 and 1/4 and back, so every `ScreenSize` changes each frame.
fn zoom(time: Res<Time>, mut cams: Query<&mut Projection, With<Camera2d>>) {
    for mut p in &mut cams {
        if let Projection::Orthographic(o) = &mut *p {
            o.scale = 0.375 + 0.125 * time.elapsed_secs().sin();
        }
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
    m.propagate.push(s.elapsed().as_secs_f64() * 1e3);
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
        .add_systems(PostUpdate, (start.before(Pixel2dSystems), mid.after(Pixel2dSystems).before(TransformSystems::Propagate), end.after(TransformSystems::Propagate)));
    let world = app.world_mut();
    for i in 0..SPRITES {
        let (x, y) = ((i % 100) as f32 * 16.0, (i / 100) as f32 * 16.0);
        let mut e = world.spawn((Transform::from_xyz(x, y, 0.0), Walker(i as f32 * 0.01)));
        if sorted {
            e.insert(YSort { layer: 1.0, offset: -8.0 });
        }
        if all && i % 10 == 0 {
            e.insert(ScreenSize::default());
        }
        e.with_child(Transform::from_xyz(0.0, -8.0, -1e-5));
    }
    if all {
        world.spawn(Camera2d);
    }
    for _ in 0..WARMUP {
        app.update();
    }
    {
        let mut m = app.world_mut().resource_mut::<Marks>();
        m.pass.clear();
        m.propagate.clear();
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
        "arm={arm} sprites={SPRITES} pass_median_ms={:.3} pass_p95_ms={:.3} propagate_median_ms={:.3} propagate_p95_ms={:.3}",
        at(&m.pass, 50),
        at(&m.pass, 95),
        at(&m.propagate, 50),
        at(&m.propagate, 95),
    );
}
