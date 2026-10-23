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
