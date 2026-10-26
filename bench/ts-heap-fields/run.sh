#!/usr/bin/env bash
# Runs the bench in release, `$1` rounds (3 by default), appending each to results/runs.txt with
# the machine's load. A round runs the plain arms built against origin/main, then every arm built
# here, so load drift hits both trees alike. main's copy is `git archive`d into target/; both build
# into the workspace's target dir, sharing its release cache, main's binary copied aside first.
set -euo pipefail
cd "$(dirname "$0")"
root=$(git rev-parse --show-toplevel)
main="$root/target/ts-heap-fields-main"
rm -rf "$main" && mkdir -p "$main"
git -C "$root" archive origin/main Cargo.toml Cargo.lock crates vendor | tar -x -C "$main"
mkdir -p "$main/bench/ts-heap-fields" && cp -R Cargo.toml src "$main/bench/ts-heap-fields/"
(cd "$main/bench/ts-heap-fields" && cargo build --release --target-dir "$root/target" -q)
cp "$root/target/release/ts-heap-fields-bench" "$root/target/ts-heap-fields-bench-main"
cargo build --release --target-dir ../../target -q
echo "main at $(git -C "$root" rev-parse --short origin/main), branch at $(git -C "$root" rev-parse --short HEAD)" | tee -a results/runs.txt
for _ in $(seq "${1:-3}"); do
  {
    uptime
    BENCH_TREE=main "$root/target/ts-heap-fields-bench-main" plain
    BENCH_TREE=branch ../../target/release/ts-heap-fields-bench
  } | tee -a results/runs.txt
done
