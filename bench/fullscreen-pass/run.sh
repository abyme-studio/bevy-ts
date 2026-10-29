#!/usr/bin/env bash
# The arms built once in release, then run alternately so each sees one machine load:
# `bench/fullscreen-pass/run.sh [runs per arm]` (3 by default; 8 when arms are within noise).
set -euo pipefail
# In one of the machine's check slots (docs/check-slots.md).
source "$(dirname "$0")/../../scripts/slot.sh"
cd "$(dirname "$0")"
runs=${1:-3}
cargo build --release
git rev-parse --short HEAD
uptime
for i in $(seq "$runs"); do
  for arm in off passthrough grade; do
    ./target/release/fullscreen-pass-bench "$arm"
  done
done
uptime
