//! One exported-param host call's Rust half, 1,000 calls per run over a 10,000-entity world.
//!
//!   cd bench/native-params/rust && cargo bench 2>&1 | grep -E "^[a-z_]+|time:"

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use native_params_bench::*;
use std::hint::black_box;
use std::time::Duration;

const CALLS: usize = 1_000;

fn call(c: &mut Criterion) {
    let mut g = c.benchmark_group("call");
    g.throughput(Throughput::Elements(CALLS as u64));

    let (mut world, es) = native_params_bench::world(10_000);
    let mut st: ParamState = Box::new(bevy_ecs::system::SystemState::<Probe<'static, 'static>>::new(&mut world));
    g.bench_function("read", |b| {
        b.iter(|| {
            for i in 0..CALLS {
                black_box(read_call(&mut st, &world, es[i * 7 % es.len()]));
            }
        })
    });

    let (mut world, es) = native_params_bench::world(10_000);
    let mut st: ParamState = Box::new(WriteState::<Mover<'static, 'static>>::new(&mut world));
    g.bench_function("write", |b| {
        b.iter(|| {
            // One TS run: its tick, then the calls inside it.
            let last_run = world.change_tick();
            let this_run = world.increment_change_tick();
            let cell = world.as_unsafe_world_cell();
            for i in 0..CALLS {
                // SAFETY: the bench holds the world exclusively.
                black_box(unsafe { write_call(&mut st, cell, last_run, this_run, es[i * 7 % es.len()]) });
            }
        })
    });

    let (mut world, es) = native_params_bench::world(10_000);
    let mut st: ParamState = Box::new(WriteState::<Spawner<'static, 'static>>::new(&mut world));
    g.bench_function("write_commands", |b| {
        b.iter(|| {
            let last_run = world.change_tick();
            let this_run = world.increment_change_tick();
            let cell = world.as_unsafe_world_cell();
            for i in 0..CALLS {
                // SAFETY: the bench holds the world exclusively.
                black_box(unsafe { commands_call(&mut st, cell, last_run, this_run, es[i * 7 % es.len()]) });
            }
            st.downcast_mut::<WriteState<Spawner<'static, 'static>>>().unwrap().apply(&mut world);
        })
    });

    let (mut world, es) = native_params_bench::world(10_000);
    let mut st: ParamState = Box::new(bevy_ecs::system::SystemState::<Mover<'static, 'static>>::new(&mut world));
    g.bench_function("get_mut", |b| {
        b.iter(|| {
            for i in 0..CALLS {
                black_box(get_mut_call(&mut st, &mut world, es[i * 7 % es.len()]));
            }
        })
    });
    g.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(3)).warm_up_time(Duration::from_secs(1));
    targets = call
}
criterion_main!(benches);
