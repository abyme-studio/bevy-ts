#!/usr/bin/env bash
# 10k Save-marked entities saved and loaded, `runs` times (3 by default):
# `bench/world-saves/run.sh [runs]`. One arm: the numbers go in docs/saves.md.
set -euo pipefail
# In one of the machine's check slots (docs/check-slots.md).
source "$(dirname "$0")/../../scripts/slot.sh"
cd "$(dirname "$0")"
runs=${1:-3}
cargo build --release
uptime
for i in $(seq "$runs"); do
  ./target/release/world-saves-bench
done
uptime
