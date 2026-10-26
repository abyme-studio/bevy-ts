#!/usr/bin/env bash
# material-params 4.2: `bench/material-params/run.sh [runs per arm]` (3 by default). Builds the
# bench release (Bevy static), then runs every arm at N = 1, 10, 100, arms alternated so both see
# one machine load. One JSON line per run in results/runs.jsonl.
set -euo pipefail
# In one of the machine's check slots (docs/check-slots.md).
source "$(dirname "$0")/../../scripts/slot.sh"
cd "$(dirname "$0")/rust"
runs=${1:-3}
cargo build --release
uptime
for i in $(seq "$runs"); do
  for n in 1 10 100; do
    for arm in set rekey; do
      ./target/release/material-params-bench "$arm" "$n" 2>/dev/null | sed -n "s/^bench: //p" | tee -a ../results/runs.jsonl
    done
  done
done
uptime
