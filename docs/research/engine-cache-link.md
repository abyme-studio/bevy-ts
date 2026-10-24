# A shared engine's first launch, copied and linked

Tasks 1.1 and 5.1 of `openspec/changes/engine-cache-link`. macOS scans every new executable path
before its first launch. Until this change a shared-cache hit copied the engine into each
project, so every project paid its own scan. Now it links to the shared file.

Measured 2026-10-23 on the shared Apple M5 Max with other agents building (load averages in the
table), debug cli, with `scripts/engine-cache-link-bench.sh` (the `ecl-bench-before` and
`ecl-bench-after` processes). Each arm builds one ports-only project engine (Rapier, 639 MB),
then three times: publishes a fresh copy into a new shared cache directory, a path never run, and
runs `bevy-ts types` in project A, in project B (same hash, same `engineCache`) and in B again.
`types` is the first thing `bevy-ts dev` runs on an engine, so its time is the first launch.

## Before: copied

| run | load | A first, s | B first, s | B second, s |
|---:|---:|---:|---:|---:|
| 1 | 42 | 260.8 | 227.2 | 0.34 |
| 2 | 32 | 262.2 | 234.4 | 0.19 |
| 3 | 31 | 180.3 | 257.6 | 0.17 |

B pays the whole scan again for bytes the machine scanned minutes earlier. The scan is slower
than the 57-110 s the proposal measured because the engine is larger and the machine was loaded.

## After: linked

| run | load | A first, s | B first, s | B second, s |
|---:|---:|---:|---:|---:|
| 1 | 22 | 138.4 | 24.9 | 0.20 |
| 2 | 26 | 111.7 | 22.7 | 0.17 |
| 3 | 27 | 126.6 | 52.3 | 0.25 |

B's local engine is a link to the shared file, and running the linked engine directly takes
0.26 s: the scan is gone. A still pays it once, on the shared path.

What is left of B's first launch is the SHA-256 of the 639 MB shared file, which bevy-ts checks
before it links. `sample` puts all of it in `sha2`'s software `compress` (22 s of CPU in the debug
cli). On aarch64 `sha2` 0.10 uses the CPU's SHA instructions only with its `asm` feature, which
this workspace does not enable; `openssl dgst -sha256`, which does use them, hashes the same file
in 0.5 s, and `shasum -a 256` in 2.9 s. The copy path paid the same hash before this change, on
top of the scan. With hardware SHA-256, B's first launch would be about the hash, 0.5 s, plus the
0.26 s start.

## The hash, with the CPU's SHA instructions

Measured 2026-10-24, same machine, with `examples/sha256_bench.rs` (the `ecl-sha` process): the
cli's `sha256_file` on the 639 MB engine, three times, in the dev profile.

| build | load | 1, s | 2, s | 3, s |
|---|---:|---:|---:|---:|
| software `compress`, unoptimized | 60 | 17.7 | 23.7 | 45.8 |
| `asm`, unoptimized | 41 | 17.1 | 12.9 | 5.7 |
| `asm`, `opt-level = 3` | 22-83 | 2.2 | 1.7 | 2.4 |

`asm` alone was not enough. The workspace builds dependencies at `opt-level = 3` in dev, but this
machine's `~/.cargo/config.toml` sets them to 0 and beats `Cargo.toml`, so sha2's NEON intrinsics
ran as calls (`sample` shows `vaddq_u32` and `step_by` frames inside `sha256_compress`). The repo's
`.cargo/config.toml` now builds sha2 at 3 whatever the machine says, and `asm` is on for aarch64
only (`crates/bevy-ts-cli/Cargo.toml`). `openssl dgst -sha256` takes 0.4 s on the same file.

The release cli, as `release.yml` builds it (`cargo build --release -p bevy-ts-cli`, the
`ecl-release` process: clean, 7 min 50 s, Bevy linked statically), run through the bench above:

| run | load | A first, s | B first, s | B second, s |
|---:|---:|---:|---:|---:|
| 1 | 23 | 5.7 | 1.09 | 0.38 |
| 2 | 23 | 7.6 | 0.44 | 0.15 |
| 3 | 21 | 25.9 | 1.15 | 0.52 |

B's first launch is now about a second, down from 23-52 s; the hash is no longer what it waits on.
