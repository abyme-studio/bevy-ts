#!/usr/bin/env bash
# Both arms built in release, then run alternately so both see one machine load:
# `bench/standard-material-features/run.sh [runs per arm]` (3 by default).
set -euo pipefail
# In one of the machine's check slots (docs/check-slots.md).
source "$(dirname "$0")/../../scripts/slot.sh"
cd "$(dirname "$0")"
runs=${1:-3}
cargo build --release --features textures && cp target/release/standard-material-features-bench target/bench-on
cargo build --release && cp target/release/standard-material-features-bench target/bench-off
./target/bench-on limits | grep pbr_
./target/bench-off limits | grep pbr_
uptime
for i in $(seq "$runs"); do
  ./target/bench-on
  ./target/bench-off
done
uptime
