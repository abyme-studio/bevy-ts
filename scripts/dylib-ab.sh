#!/bin/bash
# docs/build-cache.md, "Bevy as a dylib for tests": does a release test binary that links the
# Bevy dylib time the same as a static one? Builds the release cli test binary of <static ref> and
# of <dylib ref>, each in an export of its own, then runs levels_bench in both, alternating.
# Usage: scripts/dylib-ab.sh <static ref> <dylib ref> [runs per arm, default 3]
source "$(dirname "$0")/slot.sh"
set -uo pipefail
cd "$(dirname "$0")/.."
runs=${3:-3}
root=${TMPDIR:-/tmp}/bevy-ts-dylib-ab
mkdir -p "$root"
build() { # $1 arm, $2 ref; a binary already built from that commit is kept
  local src=$root/$1
  if [ "$(cat "$root/$1.ref" 2>/dev/null)" = "$(git rev-parse "$2")" ] && [ -x "$src/$(cat "$root/$1.exe")" ]; then
    echo "$1: $(git rev-parse --short "$2"), built"; return
  fi
  rm -rf "$src/crates" && mkdir -p "$src"
  git archive "$2" | tar -x -C "$src"
  echo "$1: $(git rev-parse --short "$2")"
  (cd "$src" && cargo test --release -p bevy-ts-cli --test it --no-run 2>&1 | grep -o "target/release/deps/it-[0-9a-f]*" | head -1) >"$root/$1.exe"
  [ -s "$root/$1.exe" ] || { echo "$1: build failed"; exit 1; }
  git rev-parse "$2" >"$root/$1.ref"
}
build static "$1"
build dylib "$2"
# What cargo gives a test it runs: the deps folder, and the toolchain's libstd.
export DYLD_FALLBACK_LIBRARY_PATH=$root/dylib/target/release/deps:$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | awk '/^host/{print $2}')/lib
for i in $(seq "$runs"); do
  for arm in static dylib; do
    echo "$arm run $i, load $(sysctl -n vm.loadavg | awk '{print $2}')"
    (cd "$root/$arm/crates/bevy-ts-cli" && "$root/$arm/$(cat "$root/$arm.exe")" levels_bench:: --ignored --nocapture 2>&1 |
      grep -E "^[0-9]+: |^\| [0-9]|panicked|dyld" | sed "s/^/$arm /")
  done
done
