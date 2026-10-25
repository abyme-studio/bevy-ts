#!/usr/bin/env bash
# The bench built in release, then run: `bench/script-image-bytes/run.sh [runs per arm]` (3 by
# default; 8 when arms are within noise). Each run appends to results/runs.txt.
set -euo pipefail
# In one of the machine's check slots (docs/check-slots.md).
source "$(dirname "$0")/../../scripts/slot.sh"
cd "$(dirname "$0")"
cargo build --release
{
  uptime
  RUNS=${1:-3} ./target/release/script-image-bytes-bench
  uptime
} | tee -a results/runs.txt
