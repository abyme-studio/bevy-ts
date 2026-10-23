#!/bin/bash
# docs/build-cache.md, "Test binaries": what one cold build and the test runs cost this worktree.
# <ref> is exported to a scratch folder, so the worktree stays free to edit: there a cold
# `cargo build --workspace --all-targets`, then the workspace tests <runs> times, warm.
# Prints target/ size, linked test binaries, duplicate Bevy builds, wall times and peak load.
# Usage: scripts/target-bench.sh <arm label> <git ref> [test runs, default 3]
set -uo pipefail
source "$(dirname "$0")/slot.sh"
arm=${1:?arm label}
ref=${2:?git ref}
runs=${3:-3}
cd "$(dirname "$0")/.."
mkdir -p bench/results
results=$PWD/bench/results
log=$results/target-$arm.log
src=${TMPDIR:-/tmp}/bevy-ts-target-bench/$arm
rm -rf "$src" && mkdir -p "$src"
git archive "$ref" | tar -x -C "$src"
cd "$src"
echo "$arm: $(git -C "$OLDPWD" rev-parse --short "$ref")"
: >"$log"
load() { sysctl -n vm.loadavg | awk '{print $2}'; }
# Samples the 1-minute load average into $1 every 5 s until killed.
sample() { while :; do load >>"$1"; sleep 5; done; }
peak() { sort -n "$1" | tail -1; }
tests() {
  # Before the viewport tests took a render lock (render_app), they ran alone, with a Bevy of their own.
  if grep -qs render_app crates/bevy-ts-viewport/tests/it/common/mod.rs; then
    cargo test --workspace --no-fail-fast >>"$log" 2>&1
  else
    cargo test --workspace --exclude bevy-ts-viewport >>"$log" 2>&1 &&
      cargo test -p bevy-ts-viewport -- --test-threads=1 >>"$log" 2>&1
  fi
}

rm -rf target
build_load=$(mktemp)
sample "$build_load" &
s=$!
t0=$SECONDS
cargo build --workspace --all-targets >>"$log" 2>&1 || echo "$arm: build failed, $log"
echo "$arm build: $((SECONDS - t0)) s, peak load $(peak "$build_load")"
kill $s; wait $s 2>/dev/null

for i in $(seq "$runs"); do
  test_load=$(mktemp)
  sample "$test_load" &
  s=$!
  t0=$SECONDS
  if tests; then ok=passed; else ok=FAILED; fi
  echo "$arm test run $i: $((SECONDS - t0)) s, $ok, peak load $(peak "$test_load")"
  kill $s; wait $s 2>/dev/null
done

cargo test --workspace --no-run --message-format=json 2>/dev/null |
  jq -r 'select(.reason == "compiler-artifact" and .profile.test == true) | "\(.target.kind[0]) \(.executable)"' |
  sort -u >"$results/target-$arm-bins.txt"
echo "$arm: $(grep -c '^test ' "$results/target-$arm-bins.txt") integration-test binaries, $(wc -l <"$results/target-$arm-bins.txt" | tr -d ' ') test binaries in all"
echo "$arm: $(ls target/debug/deps/libbevy_internal-*.rlib 2>/dev/null | wc -l | tr -d ' ') bevy_internal builds, $(ls target/debug/deps/libdeno_core-*.rlib 2>/dev/null | wc -l | tr -d ' ') deno_core builds"
echo "$arm: target $(du -sh target | cut -f1), deps $(du -sh target/debug/deps | cut -f1), incremental $(du -sh target/debug/incremental 2>/dev/null | cut -f1)"
grep -E "^test result" "$log" | awk '{p += $4; f += $6} END {print "'"$arm"': " p " tests passed, " f " failed"}'
