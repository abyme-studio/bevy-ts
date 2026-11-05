#!/usr/bin/env bash
# One level write's time by step, at 1,000 and 10,000 instances (2,000 and 20,000 entities):
# `bench/edit-split/run.sh [runs]` (3 by default). Each run is a fresh process; the table is the
# medians of each run's medians, printed last. Results go in docs/research/touched-path-reconcile.md.
set -euo pipefail
# In one of the machine's check slots (docs/check-slots.md).
source "$(dirname "$0")/../../scripts/slot.sh"
cd "$(dirname "$0")"
runs=${1:-3}
# The workspace's lockfile as the start, so the bench resolves the engine's versions.
[ -f Cargo.lock ] || cp ../../Cargo.lock Cargo.lock
# No sccache: its server drops under a loaded machine's builds.
export RUSTC_WRAPPER=""
cargo build -q --release
mkdir -p results
out=results/runs.txt
: > "$out"
uptime | tee -a "$out"
for i in $(seq "$runs"); do
  ./target/release/edit-split-bench | tee -a "$out"
done
uptime | tee -a "$out"
./target/release/edit-split-bench --summary "$out"
