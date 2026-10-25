//! A frame with no gizmos against a frame drawing 1,000 lines through `Gizmos`, the Rust baseline
//! for the TS arms of bench/gizmos/bench.mjs.
//!
//!   cd bench/gizmos/rust && cargo bench 2>&1 | grep -E "^frame|time:"

use bevy::gizmos::GizmoPlugin;
use bevy::prelude::*;
use criterion::{Criterion, criterion_group, criterion_main};

const LINES: usize = 1_000;

fn draw(mut gizmos: Gizmos) {
    for k in 0..LINES {
        let x = k as f32 * 0.01;
        gizmos.line(Vec3::new(x, 0.0, 0.0), Vec3::new(x, 1.0, 0.0), Color::srgb(1.0, 0.0, 0.0));
    }
}

fn app(lines: bool) -> App {
    let mut app = App::new();
    // GizmoPlugin's skinned-mesh bounds systems read these assets, as the engine has them.
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .add_plugins(GizmoPlugin);
    if lines {
        app.add_systems(Update, draw);
    }
    app.finish();
    app.cleanup();
    app
}

fn frame(c: &mut Criterion) {
    let mut g = c.benchmark_group("frame");
    let mut empty = app(false);
    g.bench_function("empty", |b| b.iter(|| empty.update()));
    let mut lines = app(true);
    g.bench_function("lines_1000", |b| b.iter(|| lines.update()));
    g.finish();
}

criterion_group!(benches, frame);
criterion_main!(benches);
