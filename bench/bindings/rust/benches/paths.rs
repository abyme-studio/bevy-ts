//! The Rust half of each path, in a real `bevy_ecs` 0.20 world of N entities (Alarm, Light,
//! 112 bytes of ballast) in one table, k of which changed this frame.
//!
//!   cd bench/bindings/rust && cargo bench 2>&1 | tee ../results/rust-criterion.txt | grep -E "^[a-z_]+/|time:"
//!
//! binding path (tsx design decision 9, then the patch ops the host call returns):
//!   tick_scan           N subscriptions in spawn order: `get_change_ticks_by_id` per pair, collect the changed
//!   tick_scan_shuffled  the same with subscriptions in random order (random access into the table)
//!   patch_apply         k patch ops: node id -> HashMap -> Entity -> `get_mut_by_id`, write 4 bytes, mark changed
//! system path (a TS system is the same loop in JS over the same columns; this is the Rust floor):
//!   mark_k              k random `get_mut::<Alarm>` writes: the "something changed the alarm" step both paths share
//!   system_changed      mark_k + `Query<(&Alarm, &mut Light), Changed<Alarm>>` over the table (subtract mark_k)
//!   system_all          `Query<(&Alarm, &mut Light)>` writing every row, no change check

use bevy_ecs::change_detection::{DetectChangesMut, Tick};
use bevy_ecs::component::ComponentId;
use bevy_ecs::prelude::*;
use bevy_platform::collections::HashMap;
use bindings_bench::*;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

const ROWS: [usize; 3] = [1_000, 10_000, 100_000];
const FRACS: [f64; 4] = [0.001, 0.01, 0.1, 1.0];

fn k_of(n: usize, frac: f64) -> usize {
    ((n as f64 * frac).round() as usize).max(1)
}

fn id_n(n: usize) -> String {
    format!("{}k", n / 1000)
}

fn id_nk(n: usize, frac: f64) -> String {
    format!("{}k/{}%", n / 1000, frac * 100.0)
}

fn tick_scan(world: &World, subs: &[Entity], cid: ComponentId, last: Tick, this: Tick, out: &mut Vec<u32>) {
    out.clear();
    for (i, &e) in subs.iter().enumerate() {
        if let Ok(er) = world.get_entity(e) {
            if let Some(t) = er.get_change_ticks_by_id(cid) {
                if t.is_changed(last, this) {
                    out.push(i as u32);
                }
            }
        }
    }
}

fn patch_apply(world: &mut World, map: &HashMap<u64, Entity>, ops: &[(u64, f32)], cid: ComponentId) {
    for &(node, v) in ops {
        let Some(&e) = map.get(&node) else { continue };
        if let Some(mut m) = world.get_mut_by_id(e, cid) {
            // SAFETY: `Light` is repr(C) and `intensity` is its first f32.
            unsafe { *m.as_mut().as_ptr().cast::<f32>() = v };
            m.set_changed();
        }
    }
}

fn mark_k(world: &mut World, targets: &[Entity]) {
    world.increment_change_tick();
    for &e in targets {
        if let Some(mut a) = world.get_mut::<Alarm>(e) {
            a.on = 1.0 - a.on;
        }
    }
}

fn sys_changed(mut q: Query<(&Alarm, &mut Light), Changed<Alarm>>) {
    for (a, mut l) in &mut q {
        l.intensity = if a.on > 0.0 { 8.0 } else { 2.0 };
    }
}

fn sys_all(mut q: Query<(&Alarm, &mut Light)>) {
    for (a, mut l) in &mut q {
        l.intensity = if a.on > 0.0 { 8.0 } else { 2.0 };
    }
}

fn paths(c: &mut Criterion) {
    for n in ROWS {
        let Setup { mut world, entities, shuffled, alarm_id, light_id } = setup(n);
        let changed_sys = world.register_system(sys_changed);
        let all_sys = world.register_system(sys_all);
        let map: HashMap<u64, Entity> =
            entities.iter().enumerate().map(|(i, &e)| (0x1000 + i as u64, e)).collect();
        let index_of: HashMap<Entity, u64> =
            entities.iter().enumerate().map(|(i, &e)| (e, 0x1000 + i as u64)).collect();

        // Quiet-frame scan: 1% of alarms changed since the last check.
        let last = world.increment_change_tick();
        mark_k(&mut world, &shuffled[..k_of(n, 0.01)]);
        let this = world.increment_change_tick();
        let mut out = Vec::with_capacity(n);

        let mut g = c.benchmark_group("tick_scan");
        g.throughput(Throughput::Elements(n as u64));
        g.bench_with_input(BenchmarkId::from_parameter(id_n(n)), &n, |b, _| {
            b.iter(|| tick_scan(&world, &entities, alarm_id, last, this, &mut out));
        });
        g.finish();

        let mut g = c.benchmark_group("tick_scan_shuffled");
        g.throughput(Throughput::Elements(n as u64));
        g.bench_with_input(BenchmarkId::from_parameter(id_n(n)), &n, |b, _| {
            b.iter(|| tick_scan(&world, &shuffled, alarm_id, last, this, &mut out));
        });
        g.finish();

        let mut g = c.benchmark_group("system_all");
        g.throughput(Throughput::Elements(n as u64));
        g.bench_with_input(BenchmarkId::from_parameter(id_n(n)), &n, |b, _| {
            b.iter(|| world.run_system(all_sys).unwrap());
        });
        g.finish();

        for frac in FRACS {
            let k = k_of(n, frac);
            let targets: Vec<Entity> = shuffled[..k].to_vec();
            let ops: Vec<(u64, f32)> = targets.iter().map(|e| (index_of[e], 8.0)).collect();
            let name = id_nk(n, frac);

            let mut g = c.benchmark_group("patch_apply");
            g.throughput(Throughput::Elements(k as u64));
            g.bench_with_input(BenchmarkId::from_parameter(&name), &k, |b, _| {
                b.iter(|| {
                    world.increment_change_tick();
                    patch_apply(&mut world, &map, &ops, light_id);
                });
            });
            g.finish();

            let mut g = c.benchmark_group("mark_k");
            g.throughput(Throughput::Elements(k as u64));
            g.bench_with_input(BenchmarkId::from_parameter(&name), &k, |b, _| {
                b.iter(|| mark_k(&mut world, &targets));
            });
            g.finish();

            let mut g = c.benchmark_group("system_changed");
            g.throughput(Throughput::Elements(n as u64));
            g.bench_with_input(BenchmarkId::from_parameter(&name), &k, |b, _| {
                b.iter(|| {
                    mark_k(&mut world, &targets);
                    world.run_system(changed_sys).unwrap();
                });
            });
            g.finish();
        }
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().warm_up_time(Duration::from_millis(300)).measurement_time(Duration::from_secs(2)).sample_size(30);
    targets = paths
}
criterion_main!(benches);
