//! How long the cli takes to hash an engine before it links it from a shared cache
//! (docs/research/engine-cache-link.md): `sha256_file` on each path given, three times each.
//! `cargo run -p bevy-ts-cli --example sha256_bench -- <file>...`.

use std::time::Instant;

fn main() {
    for path in std::env::args().skip(1) {
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        for _ in 0..3 {
            let t = Instant::now();
            let sum = bevy_ts_cli::engine::sha256_file(path.as_ref()).unwrap();
            let s = t.elapsed().as_secs_f64();
            println!("{path}: {s:.2} s, {:.0} MB/s, {}", size as f64 / 1e6 / s, &sum[..12]);
        }
    }
}
