//! Post-run passes for each change-detection option, at 10k / 100k / 1M rows and
//! 0.3% / 10% / 100% of rows written.
//!
//!   cd bench/change-detection/rust && cargo bench 2>&1 | grep -E "^[a-z_]+/|time:"

use change_detection_bench::*;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::time::Duration;

const ROWS: [usize; 3] = [10_000, 100_000, 1_000_000];
const FRACS: [f64; 3] = [0.003, 0.1, 1.0];
const TICK: u32 = 1_234_567;

fn id(n: usize, frac: f64) -> String {
    format!("{}k/{}%", n / 1000, frac * 100.0)
}

fn post_run(c: &mut Criterion) {
    for n in ROWS {
        for frac in FRACS {
            let mut col = Column::new(n, frac);
            let name = id(n, frac);

            let mut g = c.benchmark_group("mark_all");
            g.throughput(Throughput::Elements(n as u64));
            g.bench_with_input(BenchmarkId::from_parameter(&name), &n, |b, _| {
                b.iter(|| mark_all(&mut col.ticks, TICK));
            });
            g.finish();

            let bits = bits_from_want(&col.want);
            let mut g = c.benchmark_group("bits_to_ticks");
            g.throughput(Throughput::Elements(n as u64));
            g.bench_with_input(BenchmarkId::from_parameter(&name), &n, |b, _| {
                b.iter(|| bits_to_ticks(&bits, &mut col.ticks, TICK));
            });
            g.finish();

            let mut shadow = vec![0u32; n * STRIDE];
            let mut g = c.benchmark_group("shadow_copy");
            g.throughput(Throughput::Elements(n as u64));
            g.bench_with_input(BenchmarkId::from_parameter(&name), &n, |b, _| {
                b.iter(|| shadow_copy(&col.data, &mut shadow));
            });
            g.finish();

            // The run wrote x on every wanted row; the shadow holds the old column.
            shadow_copy(&col.data, &mut shadow);
            for i in 0..n {
                if col.want[i] == 1 {
                    col.data[i * STRIDE] ^= 0x4000_0000;
                }
            }
            let mut g = c.benchmark_group("shadow_diff");
            g.throughput(Throughput::Elements(n as u64));
            g.bench_with_input(BenchmarkId::from_parameter(&name), &n, |b, _| {
                b.iter(|| shadow_diff(&col.data, &shadow, &mut col.ticks, TICK));
            });
            g.finish();

            let mut g = c.benchmark_group("shadow_diff_pages");
            g.throughput(Throughput::Elements(n as u64));
            g.bench_with_input(BenchmarkId::from_parameter(&name), &n, |b, _| {
                b.iter(|| shadow_diff_pages(&col.data, &shadow, &mut col.ticks, TICK));
            });
            g.finish();

            let ops = cmds_from_want(&col.want);
            let mut g = c.benchmark_group("apply_cmds");
            g.throughput(Throughput::Elements(n as u64));
            g.bench_with_input(BenchmarkId::from_parameter(&name), &n, |b, _| {
                b.iter(|| apply_cmds(&ops, &mut col.data, &mut col.ticks, TICK));
            });
            g.finish();

            // Ticks as option B leaves them: written rows at TICK, others older.
            for i in 0..n {
                col.ticks[i] = if col.want[i] == 1 { TICK } else { TICK - 100 };
            }
            let mut g = c.benchmark_group("changed_scan");
            g.throughput(Throughput::Elements(n as u64));
            g.bench_with_input(BenchmarkId::from_parameter(&name), &n, |b, _| {
                b.iter(|| changed_scan(&col.ticks, TICK - 1, TICK + 1));
            });
            g.finish();

            let mut g = c.benchmark_group("hash_column");
            g.throughput(Throughput::Elements(n as u64));
            g.bench_with_input(BenchmarkId::from_parameter(&name), &n, |b, _| {
                b.iter(|| hash_column(&col.data));
            });
            g.finish();
        }
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_millis(300))
        .measurement_time(Duration::from_millis(1500))
        .sample_size(20);
    targets = post_run
}
criterion_main!(benches);
