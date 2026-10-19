//! What one slot costs the parallel executor (native task 4.4): Update with Rust systems on both
//! sides of an exposed set, run with no slot, with an empty exclusive slot after the set, and
//! with a slot whose run condition skips it. Release build:
//! `cargo run --release -p bevy-ts-cli --example slot_barrier`.

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use std::time::Instant;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct Step;

#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct Bench;

/// Enough work per system that the executor runs them in parallel.
fn work(mut acc: Local<Vec<f32>>) {
    if acc.is_empty() {
        acc.resize(20_000, 1.0);
    }
    let mut s = 0.0;
    for (i, v) in acc.iter_mut().enumerate() {
        *v = (*v * 1.0001 + i as f32).sqrt();
        s += *v;
    }
    std::hint::black_box(s);
}

fn slot(world: &mut World) {
    std::hint::black_box(world.change_tick());
}

fn never() -> bool {
    false
}

#[derive(Clone, Copy, Debug)]
enum Slot {
    None,
    Empty,
    Skipped,
}

fn run(variant: Slot, frames: u32) -> f64 {
    let mut world = World::new();
    let mut schedule = Schedule::new(Bench);
    for _ in 0..8 {
        schedule.add_systems(work.before(Step));
        schedule.add_systems(work.in_set(Step));
        schedule.add_systems(work.after(Step));
    }
    match variant {
        Slot::None => {}
        Slot::Empty => {
            schedule.add_systems(slot.after(Step));
        }
        Slot::Skipped => {
            schedule.add_systems(slot.after(Step).run_if(never));
        }
    }
    for _ in 0..50 {
        schedule.run(&mut world);
    }
    let t = Instant::now();
    for _ in 0..frames {
        schedule.run(&mut world);
    }
    t.elapsed().as_secs_f64() * 1e6 / frames as f64
}

fn main() {
    let frames = 2_000;
    println!("| slot | µs per schedule run (median of 5) |\n|---|---|");
    for v in [Slot::None, Slot::Empty, Slot::Skipped] {
        let mut samples: Vec<f64> = (0..5).map(|_| run(v, frames)).collect();
        samples.sort_by(f64::total_cmp);
        println!("| {v:?} | {:.1} |", samples[2]);
    }
}
