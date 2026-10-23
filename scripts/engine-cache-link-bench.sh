#!/usr/bin/env bash
# Change engine-cache-link, tasks 1.1 and 5.1: the first launch of a shared-cache engine.
#
# One ports-only project engine is built once. Each run publishes a fresh copy of it into a new
# shared cache directory (a path macOS has never scanned), then runs `bevy-ts types` in project A,
# in project B (same hash, same engineCache) and in B again. `types` is the first thing
# `bevy-ts dev` executes on an engine, so its time is the first launch.
#
# ECL_ARM=before|after picks target/ecl/bevy-ts-<arm>; ECL_RUNS (3) runs per arm.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
arm=${ECL_ARM:?ECL_ARM=before or after}
runs=${ECL_RUNS:-3}
bin=$root/target/ecl/bevy-ts-$arm
work=$root/target/ecl/$arm
export CARGO_TARGET_DIR=$root/target

project() {
  mkdir -p "$1/src"
  printf '{ "native": { "ports": ["rapier3d"] }%s }\n' "${2:+, \"engineCache\": \"$2\"}" > "$1/bevy-ts.json"
  echo 'export const x = 1;' > "$1/src/game.ts"
}

secs() { python3 -c 'import time; print(f"{time.time():.3f}")'; }

timed() {
  local t0 t1
  t0=$(secs)
  (cd "$1" && "$bin" types > /dev/null 2> "$1.log") || { echo "$2 failed:"; cat "$1.log"; exit 1; }
  t1=$(secs)
  python3 -c "print(f'$2 {$t1 - $t0:.2f} s')"
  grep -hE '^bevy-ts: .*(shared cache|link)' "$1.log" | sed 's/^/  /' || true
}

mkdir -p "$work"
"$bin" > /dev/null 2>&1 || true # the cli's own first-launch scan, outside the timings

# The engine, once per arm: the hash covers the cli binary, so each arm builds its own.
rm -rf "$work/p0"
project "$work/p0"
(cd "$work/p0" && "$bin" types > /dev/null)
eng=$(ls -d "$work"/p0/.bevy-ts/engines/*/ | head -1)
hash=$(basename "$eng")
echo "arm $arm, engine $hash, $(du -h "$eng/bevy-ts" | cut -f1)"

for i in $(seq "$runs"); do
  uptime
  run=$work/run-$i-$RANDOM
  cache=$run/cache
  mkdir -p "$cache/$hash.tmp/$(rustc -vV | sed -n 's/^host: //p')"
  entry=$(ls -d "$cache/$hash.tmp"/*/)
  cp "$eng/bevy-ts" "$eng/bevy-ts.sha256" "$entry"
  mv "$cache/$hash.tmp" "$cache/$hash"
  project "$run/a" "$cache"
  project "$run/b" "$cache"
  echo "run $i"
  timed "$run/a" "  A first"
  timed "$run/b" "  B first"
  timed "$run/b" "  B second"
  ls -l "$run/b/.bevy-ts/engines/$hash/" | sed -n 's/^\(.\).* \(bevy-ts[^ ]*\)\( -> .*\)\{0,1\}$/  b: \1 \2\3/p'
done
uptime
