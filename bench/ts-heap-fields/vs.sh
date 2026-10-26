#!/usr/bin/env bash
# Every arm built at commit `$1` against every arm built here, `$2` rounds (8 by default),
# interleaved so load drift hits both alike; appended to results/runs.txt. The ref's own bench
# source is built, in a `git archive` of it under target/, as run.sh builds main's.
set -euo pipefail
cd "$(dirname "$0")"
root=$(git rev-parse --show-toplevel)
ref=$(git -C "$root" rev-parse --short "$1")
dir="$root/target/ts-heap-fields-ref"
rm -rf "$dir" && mkdir -p "$dir"
git -C "$root" archive "$ref" Cargo.toml Cargo.lock crates vendor protocol packages/bevy-ts bench/ts-heap-fields | tar -xm -C "$dir"
printf '\n[workspace]\n' >>"$dir/bench/ts-heap-fields/Cargo.toml"
(cd "$dir/bench/ts-heap-fields" && cargo build --release --target-dir "$root/target" -q)
cp "$root/target/release/ts-heap-fields-bench" "$root/target/ts-heap-fields-bench-ref"
cargo build --release --target-dir ../../target -q
cp ../../target/release/ts-heap-fields-bench "$root/target/ts-heap-fields-bench-branch"
echo "$ref at every arm, branch at $(git -C "$root" rev-parse --short HEAD)" | tee -a results/runs.txt
for _ in $(seq "${2:-8}"); do
  {
    uptime
    BENCH_TREE="$ref" "$root/target/ts-heap-fields-bench-ref"
    BENCH_TREE=branch "$root/target/ts-heap-fields-bench-branch"
  } | tee -a results/runs.txt
done
