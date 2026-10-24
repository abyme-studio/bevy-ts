#!/usr/bin/env bash
# ecs-schedules task 2.1: build each arm (16, 32 slots) timing the bench crate's own compile, then
# run the 32-slot bench $1 times (3 unless given). Results go to stdout; the process tees them.
set -euo pipefail
cd "$(dirname "$0")"
runs="${1:-3}"
cargo build --release -q 2>/dev/null || cargo build --release
for arm in slots16 slots32; do
  flags=""; [ "$arm" = slots16 ] && flags="--features slots16"
  cargo clean --release -q -p states-bench
  start=$(python3 -c 'import time;print(time.time())')
  cargo build --release -q $flags
  end=$(python3 -c 'import time;print(time.time())')
  size=$(stat -f %z target/release/states-bench 2>/dev/null || stat -c %s target/release/states-bench)
  python3 -c "print(f'build $arm: {${end}-${start}:.1f} s, binary {${size}/1e6:.2f} MB')"
  cp target/release/states-bench "target/states-bench-$arm"
done
for i in $(seq 1 "$runs"); do
  echo "run $i, $(uptime | sed 's/.*load/load/')"
  target/states-bench-slots32
  SINGLE=1 target/states-bench-slots32
done
