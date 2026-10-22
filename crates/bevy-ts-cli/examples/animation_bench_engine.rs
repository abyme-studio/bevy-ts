//! The engine `tests/animation_bench.rs` runs (change `scene-animation`, task 6.1): the prebuilt
//! engine plus a probe, so every arm runs the same binary. `ANIMATION_BENCH_ARM` and
//! `ANIMATION_BENCH_N` select the run; the probe writes its numbers to `ANIMATION_BENCH_OUT`.
//!
//! - Arm `A` also spawns N foxes the way Bevy's `many_foxes` does: its own observer, graph and
//!   transitions, no TS and no `PlayAnimation`.
//! - Arm `D` (a diagnostic) spawns them as A does and plays them with `PlayAnimation`.
//! - Arms `B` and `C` spawn their foxes from TSX; this engine only measures.
//!
//! The probe: each update's main-world time (`First` to `Last`), and the frames and time from the
//! frame N foxes exist to the frame all N players have an active animation.
//! `cargo build --release -p bevy-ts-cli --example animation_bench_engine`

use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex};
use bevy::animation::AnimationPlayer;
use bevy::animation::transition::AnimationTransitions;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use std::time::{Duration, Instant};

/// Frames after readiness before sampling: pipelines compile, textures upload.
const WARMUP: u32 = 480;
/// Updates sampled: 10 s at 120 Hz.
const SAMPLES: usize = 1200;

#[derive(Resource)]
struct Bench {
    arm: String,
    n: usize,
    out: std::path::PathBuf,
    frame: u32,
    started: Option<Instant>,
    spawned: Option<(u32, Instant)>,
    ready: Option<(u32, Instant)>,
    samples: Vec<f64>,
    changed: usize,
    done: bool,
}

/// Arm A's foxes and their one graph, as `many_foxes` keeps them.
#[derive(Resource)]
struct Foxes {
    graph: Handle<AnimationGraph>,
    run: AnimationNodeIndex,
}

#[derive(Component)]
struct RustFox;

struct BenchPlugin;

impl Plugin for BenchPlugin {
    fn build(&self, app: &mut App) {
        let Ok(arm) = std::env::var("ANIMATION_BENCH_ARM") else { return };
        let n = std::env::var("ANIMATION_BENCH_N").ok().and_then(|s| s.parse().ok()).unwrap_or(100);
        let out = std::env::var("ANIMATION_BENCH_OUT").expect("ANIMATION_BENCH_OUT").into();
        app.insert_resource(Bench { arm: arm.clone(), n, out, frame: 0, started: None, spawned: None, ready: None, samples: vec![], changed: 0, done: false })
            .add_systems(First, start_update)
            .add_systems(Last, end_update);
        if arm == "A" {
            app.add_systems(Startup, spawn_foxes).add_observer(play_fox);
        }
        if arm == "D" {
            app.add_systems(Startup, spawn_played_foxes);
        }
    }
}

/// The grid the TSX arms use: `side` x `side`, 2.5 apart, centred.
fn grid(n: usize) -> Vec<Vec3> {
    let side = (n as f32).sqrt().ceil() as usize;
    let offset = (side as f32 - 1.0) * 2.5 / 2.0;
    (0..n).map(|i| Vec3::new((i % side) as f32 * 2.5 - offset, 0.0, (i / side) as f32 * 2.5 - offset)).collect()
}

fn spawn_foxes(mut commands: Commands, bench: Res<Bench>, server: Res<AssetServer>, mut graphs: ResMut<Assets<AnimationGraph>>) {
    let (graph, run) = AnimationGraph::from_clip(server.load("models/Fox.glb#Animation2"));
    commands.insert_resource(Foxes { graph: graphs.add(graph), run });
    let scene = server.load("models/Fox.glb#Scene0");
    for at in grid(bench.n) {
        commands.spawn((RustFox, WorldAssetRoot(scene.clone()), Transform::from_translation(at).with_scale(Vec3::splat(0.02))));
    }
}

/// Arm D, a diagnostic: Rust spawns the foxes as A does, and `PlayAnimation` plays them, so
/// D against A is the plugin's cost and D against B is TSX's.
fn spawn_played_foxes(mut commands: Commands, bench: Res<Bench>, server: Res<AssetServer>) {
    let scene = server.load("models/Fox.glb#Scene0");
    let clip = server.load("models/Fox.glb#Animation2");
    for at in grid(bench.n) {
        commands.spawn((
            WorldAssetRoot(scene.clone()),
            bevy_ts_animation::PlayAnimation { clip: clip.clone(), ..default() },
            Transform::from_translation(at).with_scale(Vec3::splat(0.02)),
        ));
    }
}

/// `many_foxes`'s wiring: once a fox's scene is ready, its player gets the graph and transitions
/// and plays the run on repeat.
fn play_fox(
    ready: On<WorldInstanceReady>,
    foxes: Query<(), With<RustFox>>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    shared: Res<Foxes>,
    mut commands: Commands,
) {
    if !foxes.contains(ready.entity) {
        return;
    }
    for e in children.iter_descendants(ready.entity) {
        if let Ok(mut player) = players.get_mut(e) {
            let mut transitions = AnimationTransitions::new();
            transitions.play(&mut player, shared.run, Duration::ZERO).repeat();
            commands.entity(e).insert((AnimationGraphHandle(shared.graph.clone()), transitions));
        }
    }
}

fn start_update(mut bench: ResMut<Bench>) {
    bench.started = Some(Instant::now());
}

fn end_update(
    mut bench: ResMut<Bench>,
    roots: Query<(), With<WorldAssetRoot>>,
    players: Query<&AnimationPlayer>,
    changed: Query<(), Changed<bevy_ts_animation::PlayAnimation>>,
) {
    let now = Instant::now();
    let took = bench.started.map(|s| (now - s).as_secs_f64() * 1000.0);
    bench.frame += 1;
    let frame = bench.frame;
    if bench.spawned.is_none() && roots.iter().count() >= bench.n {
        bench.spawned = Some((frame, now));
    }
    if bench.spawned.is_some() && bench.ready.is_none() {
        let playing = players.iter().filter(|p| p.playing_animations().next().is_some()).count();
        // Static foxes never play: their run is ready once their players exist.
        let ready = if bench.arm == "C" { players.iter().count() >= bench.n } else { playing >= bench.n };
        if ready {
            bench.ready = Some((frame, now));
        }
    }
    let Some((ready, _)) = bench.ready else { return };
    if bench.done || frame < ready + WARMUP {
        return;
    }
    if let Some(t) = took {
        bench.samples.push(t);
    }
    // Rows whose `PlayAnimation` changed in a sampled frame: none, once the clip plays.
    bench.changed += changed.iter().count();
    if bench.samples.len() >= SAMPLES {
        bench.done = true;
        let mut s = bench.samples.clone();
        s.sort_by(f64::total_cmp);
        let mean = s.iter().sum::<f64>() / s.len() as f64;
        let p95 = s[(s.len() as f64 * 0.95) as usize - 1];
        let (sf, st) = bench.spawned.unwrap();
        let (rf, rt) = bench.ready.unwrap();
        let v = serde_json::json!({
            "arm": bench.arm, "n": bench.n, "meanMs": mean, "p95Ms": p95,
            "readyFrames": rf - sf, "readyMs": (rt - st).as_secs_f64() * 1000.0,
            "players": players.iter().count(), "changedRows": bench.changed,
        });
        std::fs::write(&bench.out, v.to_string()).unwrap();
    }
}

fn main() {
    bevy_ts_cli::run(BenchPlugin);
}
