#!/usr/bin/env bash
# Change scene-animation, task 6: arms A, B and C at each N, RUNS runs each, one release engine per
# run, rows appended to target/animation-bench.md.
# scripts/animation-bench.sh [runs] [N ...]   (defaults: 3 runs, N = 50 100 200; ARMS="A B C D" adds the diagnostic arm;
# ARMS="B G P H K" 400 is change animation-control's: playing, no graph, paused, off screen, off screen culled;
# ARMS="T S" 400 its tweens: a TS-built clip against a TS system)
source "$(dirname "$0")/slot.sh"
set -uo pipefail
cd "$(dirname "$0")/.."
mkdir -p target
runs=${1:-3}
shift || true
ns=("$@")
[[ ${#ns[@]} -eq 0 ]] && ns=(50 100 200)
build() { "$@" > target/animation-bench-build.log 2>&1 || { grep -E "^error" -A 8 target/animation-bench-build.log; exit 1; }; }
build cargo static build --release -p bevy-ts-cli --example animation_bench_engine
build cargo static test --release -p bevy-ts-cli --test it animation_bench:: --no-run
export ANIMATION_BENCH_ENGINE=$PWD/target/release/examples/animation_bench_engine
echo "| arm | N | players | update mean ms | update p95 ms | ready frames | ready ms | game GPU p50 | CPU | RSS MB |" >> target/animation-bench.md
for n in "${ns[@]}"; do
  for ((r = 1; r <= runs; r++)); do
    # Arms interleaved, so drift in the machine's load spreads across them.
    for arm in ${ARMS:-A B C}; do
      ARM=$arm N=$n timeout 300 cargo static test --release -p bevy-ts-cli --test it animation_bench:: -- --ignored --nocapture 2>&1 | grep -E '^\| [ABCDPHKGTS] |panicked|assert'
    done
  done
done
