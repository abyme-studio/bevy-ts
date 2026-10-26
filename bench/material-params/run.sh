#!/usr/bin/env bash
# material-params: `bench/material-params/run.sh [runs per arm] [stage]` (3 runs, stage 2 by
# default). Builds the bench release (Bevy static), then runs the stage's arms alternated so both
# see one machine load. Stage 2 (4.2): set and rekey at N = 1, 10, 100, into results/runs.jsonl.
# Stage 3 (4.3): instance and instance-rekey at N = 10000, 150 frames, into results/runs-instance.jsonl.
set -euo pipefail
# In one of the machine's check slots (docs/check-slots.md).
source "$(dirname "$0")/../../scripts/slot.sh"
cd "$(dirname "$0")/rust"
runs=${1:-3}
stage=${2:-2}
cargo build --release
uptime
for i in $(seq "$runs"); do
  if [[ $stage == 3 ]]; then
    for arm in instance instance-rekey; do
      ./target/release/material-params-bench "$arm" 10000 150 2>/dev/null | sed -n "s/^bench: //p" | tee -a ../results/runs-instance.jsonl
    done
    continue
  fi
  for n in 1 10 100; do
    for arm in set rekey; do
      ./target/release/material-params-bench "$arm" "$n" 2>/dev/null | sed -n "s/^bench: //p" | tee -a ../results/runs.jsonl
    done
  done
done
uptime
