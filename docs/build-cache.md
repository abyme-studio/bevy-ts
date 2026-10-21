# Build cache

`.cargo/config.toml` runs rustc through `scripts/rustc-wrapper.sh`: sccache when it is
installed, plain rustc when not. sccache is not required anywhere; CI does not install it and
builds as before.

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
