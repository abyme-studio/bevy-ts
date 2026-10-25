#!/usr/bin/env bash
# Runs the bench in release, `$1` times (3 by default), appending each run to results/runs.txt
# with the machine's load. Builds into the workspace's target dir so it shares its release cache.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target-dir ../../target -q
for _ in $(seq "${1:-3}"); do
  { uptime; ../../target/release/rust-events-bench; } | tee -a results/runs.txt
done
