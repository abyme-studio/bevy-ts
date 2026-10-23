#!/bin/bash
# docs/build-cache.md: cold `cargo build --workspace --all-targets` without sccache, then with it warm.
# Usage: scripts/sccache-bench.sh [runs per arm, default 3] [arms: none, sccache, ab; default "none sccache"]
source "$(dirname "$0")/slot.sh"
set -euo pipefail
runs=${1:-3}
arms=${2:-none sccache}
cd "$(dirname "$0")/.."
log=bench/results/sccache-build.log
build() { # $1 arm, $2 run
  rm -rf target
  sccache --zero-stats >/dev/null 2>&1 || true
  local t0=$SECONDS
  cargo build --workspace --all-targets --quiet 2>>"$log" || { echo "$1 run $2: build failed, $log"; exit 1; }
  local stats
  stats=$(sccache --show-stats 2>/dev/null || true)
  local hits miss
  hits=$(awk '/^Cache hits \(Rust\)/{print $4}' <<<"$stats")
  miss=$(awk '/^Cache misses \(Rust\)/{print $4}' <<<"$stats")
  echo "$1 run $2: $((SECONDS - t0)) s, rust hits ${hits:-0}, misses ${miss:-0}, load $(sysctl -n vm.loadavg)"
}
for arm in $arms; do
  case $arm in
    none) for i in $(seq "$runs"); do RUSTC_WRAPPER= build none "$i"; done ;;
    sccache) build warmup 0; for i in $(seq "$runs"); do build sccache "$i"; done ;;
    # Alternate the arms so both see one machine load (the cache must already be warm).
    ab) for i in $(seq "$runs"); do RUSTC_WRAPPER= build none "$i"; build sccache "$i"; done ;;
  esac
done
