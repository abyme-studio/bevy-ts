#!/bin/bash
# docs/build-cache.md, "Bevy as a dylib for tests": tests link Bevy as one dylib, and nothing that
# ships does. Checks, with otool -L, a test binary (links libbevy_dylib), the release CLI and the
# dev CLI (do not), and what the release CLI builds: a game on the prebuilt engine and a project
# engine (do not). Run in a checkout; [folder] defaults to this one.
# Usage: scripts/dylib-check.sh [folder]
source "$(dirname "$0")/slot.sh"
set -uo pipefail
cd "${1:-$(dirname "$0")/..}"
ok=1
links() { otool -L "$1" 2>/dev/null | grep -c libbevy_dylib; }
check() { # $1 what, $2 binary, $3 expected: dynamic or static
  local n
  if [ ! -f "$2" ]; then echo "$1: missing $2"; ok=0; return; fi
  n=$(links "$2")
  if { [ "$3" = dynamic ] && [ "$n" -gt 0 ]; } || { [ "$3" = static ] && [ "$n" = 0 ]; }; then
    echo "$1: $3, as it should be ($2)"
  else
    echo "$1: NOT $3 ($2)"; otool -L "$2" | head -8; ok=0
  fi
}
out=${TMPDIR:-/tmp}/bevy-ts-dylib-check
rm -rf "$out"

cargo test -p bevy-ts --test it --no-run -q 2>/dev/null
check "a test binary" "$(ls -t target/debug/deps/it-* | grep -v '\.d$' | head -1)" dynamic
cargo build -q -p bevy-ts-cli 2>/dev/null
check "the dev CLI" target/debug/bevy-ts static
cargo build -q --release -p bevy-ts-cli 2>/dev/null
check "the release CLI" target/release/bevy-ts static
target/release/bevy-ts build examples/cube --out "$out/cube" >/dev/null 2>&1
check "a game on the prebuilt engine" "$(find "$out/cube" -maxdepth 1 -type f -perm -u+x | head -1)" static
target/release/bevy-ts build examples/thrusters --out "$out/thrusters" >/dev/null 2>&1
check "a project engine's game" "$(find "$out/thrusters" -maxdepth 1 -type f -perm -u+x | head -1)" static
[ $ok = 1 ] && echo "dylib-check: passed" || { echo "dylib-check: FAILED"; exit 1; }
