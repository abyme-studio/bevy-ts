#!/usr/bin/env bash
# The arms built once in release, then run alternately so each sees one machine load:
# `bench/pixel-2d/run.sh [runs per arm]` (3 by default; 8 when arms are within noise).
set -euo pipefail
# In one of the machine's check slots (docs/check-slots.md).
source "$(dirname "$0")/../../scripts/slot.sh"
cd "$(dirname "$0")"
runs=${1:-3}
cargo build --release
uptime
for i in $(seq "$runs"); do
  for arm in off ysort-still ysort-moving all-moving; do
    ./target/release/pixel-2d-bench "$arm"
  done
done
uptime
