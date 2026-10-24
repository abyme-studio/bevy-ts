//! Per-frame cost of TS state machines as Bevy states (ecs-schedules design decision 10).

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use std::time::Instant;

/// One slot of the pool: a TS machine's current value, an interned symbol.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TsState<const N: u8>(u32);

#[cfg(not(feature = "slots16"))]
const SLOTS: usize = 32;
#[cfg(feature = "slots16")]
const SLOTS: usize = 16;

/// Registers slot `N` when `N < n`: what bevy-ts does the first time a machine takes the slot.
fn register<const N: u8>(app: &mut App, n: usize) {
    if (N as usize) < n && (N as usize) < SLOTS {
        app.init_state::<TsState<N>>();
        app.world_mut().get_resource_or_init::<Flippers>().0.push(flip::<N>);
    }
}

/// Transition frames: a machine moves to its other value.
fn flip<const N: u8>(world: &mut World) {
    let v = world.resource::<State<TsState<N>>>().get().0;
    world.resource_mut::<NextState<TsState<N>>>().set(TsState(1 - v));
}

/// Each registered slot's flip, run by one exclusive system as bevy-ts's TS commands would set
/// them; quiet frames run no system at all.
#[derive(Resource, Default)]
struct Flippers(Vec<fn(&mut World)>);

fn flip_all(world: &mut World) {
    let fs = world.resource::<Flippers>().0.clone();
    for f in fs {
        f(world);
    }
}

macro_rules! register_all {
    ($app:expr, $n:expr; $($i:literal)*) => { $( register::<$i>($app, $n); )* };
}

fn app(n: usize, flip: bool) -> App {
    let mut app = App::new();
    app.add_plugins(StatesPlugin).init_resource::<Flippers>();
    register_all!(&mut app, n; 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31);
    if flip {
        app.add_systems(Update, flip_all);
    }
    // Entities scoped to a state, so the state-scoped systems have rows to look at.
    for _ in 0..64 {
        app.world_mut().spawn(DespawnOnExit(TsState::<0>(7)));
    }
    if std::env::var("SINGLE").is_ok() {
        // The arm with `StateTransition` on one thread: its systems are many and tiny.
        app.edit_schedule(bevy::state::state::StateTransition, |s| {
            s.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
        });
    }
    app.finish();
    app.cleanup();
    app
}

/// Microseconds per frame, median of 5 runs of `frames` frames after a warm-up.
fn time(n: usize, flip: bool, frames: u32) -> f64 {
    let mut app = app(n, flip);
    for _ in 0..1000 {
        app.update();
    }
    let mut runs: Vec<f64> = (0..5)
        .map(|_| {
            let t = Instant::now();
            for _ in 0..frames {
                app.update();
            }
            t.elapsed().as_secs_f64() * 1e6 / frames as f64
        })
        .collect();
    runs.sort_by(f64::total_cmp);
    runs[2]
}

fn main() {
    let frames = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(20_000);
    let exec = if std::env::var("SINGLE").is_ok() { "single-threaded" } else { "multi-threaded" };
    println!("slots compiled: {SLOTS}, frames per run: {frames}, StateTransition {exec}");
    println!("| machines | quiet µs/frame | quiet µs/machine | transition µs/frame | transition µs/machine |");
    println!("| --- | --- | --- | --- | --- |");
    let base_q = time(0, false, frames);
    let base_t = time(0, true, frames);
    for n in [0usize, 1, 8, SLOTS] {
        let (q, t) = (time(n, false, frames), time(n, true, frames));
        let per = |x: f64, b: f64| if n == 0 { "-".to_string() } else { format!("{:.3}", (x - b) / n as f64) };
        println!("| {n} | {q:.2} | {} | {t:.2} | {} |", per(q, base_q), per(t, base_t));
    }
}
