#!/usr/bin/env bash
# The three arms built once in release, then run alternately so each sees one machine load:
# `bench/world-text/run.sh [runs per arm]` (3 by default).
set -euo pipefail
# In one of the machine's check slots (docs/check-slots.md).
source "$(dirname "$0")/../../scripts/slot.sh"
cd "$(dirname "$0")"
runs=${1:-3}
cargo build --release
uptime
for i in $(seq "$runs"); do
  for arm in empty still churn; do
    ./target/release/world-text-bench "$arm"
  done
done
uptime
