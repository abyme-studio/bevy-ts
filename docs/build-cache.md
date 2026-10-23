# Build cache

The repo sets no rustc wrapper: a `.sh` wrapper broke the Windows release build, which cannot
run one. sccache is turned on per machine instead, in `~/.cargo/config.toml`:

```toml
[build]
rustc-wrapper = "sccache"
```

sccache is not required anywhere; CI does not install it and builds with plain rustc.

What it shares across worktrees is the registry's crates (Bevy, deno_core and the rest). The
workspace's own crates are incremental, which sccache passes through uncached, and so are
proc-macros, build scripts and links. It saves compile time, not disk: each worktree still has its
own `target/`.

## Measured

`scripts/sccache-bench.sh 8 ab` (process `sccache-bench-ab`), 2026-10-22: a cold
`cargo build --workspace --all-targets` in a fresh worktree, `target/` removed before each, the arms
alternated so both see one machine load (load average 28-55, other agents building). `none` sets
`RUSTC_WRAPPER=` to bypass sccache; `sccache` runs on a cache warmed by an earlier build.

| arm     | runs (s)                                | median | mean  |
| ------- | --------------------------------------- | ------ | ----- |
| none    | 251, 221, 268, 238, 202, 234, 162, 173  | 228 s  | 219 s |
| sccache | 133, 231, 177, 140, 130, 217, 130, 174  | 157 s  | 167 s |

sccache was faster in 6 of 8 pairs: about 30% off a cold build. The first build into an empty
cache cost 1203 s under a load average near 100, so a cold cache is not a free pass. Raw lines are
in `bench/results/sccache.txt`; the hit counts there are the shared sccache server's, so other
agents' builds land in them too.

The cache (`~/Library/Caches/Mozilla.sccache`) is shared with every Rust repo on the machine and
sits at its 50 GiB limit, so least recently used entries are evicted; a worktree that waits long
enough may find Bevy gone.

## Test binaries

Each crate links one integration-test binary, `it`: its tests are modules of
`crates/<crate>/tests/it/main.rs`, one file per module, with shared helpers in `tests/it/common/`.
Every binary links the whole engine (Bevy, V8, wgpu), so one per file was most of `target/`. A new
test file is a new `mod` line in `main.rs`; a file left at `tests/*.rs` builds a binary of its own.

A file's tests are named by its module, and filters still work:

```sh
cargo test -p bevy-ts-cli --test it levels_protocol::      # one file
cargo test -p bevy-ts-cli --test it -- tsx:: tsx_sites::   # several
```

A filter is a substring: `protocol::` also runs `levels_protocol::` and `tsx_protocol::`; add
`--skip levels_protocol::` to leave them out. The modules share one process, so a test takes its
engine's port from `common::free_port()`, never from the process id.
How to write one: `docs/testing.md`.

The dev and test profiles keep line tables only (`debug = "line-tables-only"`), so backtraces keep
file and line, and dependencies carry no debug info. The repo's `.cargo/config.toml` repeats the
line tables, because a machine's `~/.cargo/config.toml` that sets `debug = false` beats
`Cargo.toml`.

`scripts/target-bench.sh` (processes `target-bench-before`, `target-bench-after`), 2026-10-23: a
cold `cargo build --workspace --all-targets` on an export of the commit, then the gate's tests 3
times, warm. Load averages 27-45 throughout, from other agents' builds.

| | before (`1933a72`) | after |
| --- | --- | --- |
| `target/` | 31 GB | 12 GB |
| integration-test binaries | 59 | 9 |
| Bevy builds (`bevy_internal`) | 2 | 1 |
| cold build | 331 s | 92 s |
| test runs | 1114, 268, 302 s | 284, 217, 163 s |
| peak load, build / tests | 40 / 34-45 | 26 / 26-43 |
| tests passed | 1680 | 1998 |

The second Bevy was `cargo test -p bevy-ts-viewport`, run alone for `--test-threads=1`: a crate
selected on its own resolves Bevy's features afresh. Its render tests now take a lock
(`common::render_app`), so it runs in the one `cargo test --workspace`. The first test run of each
arm includes linking. The after arm (`62958e8`) is on a later main, with 14 more test files and a crate more.
