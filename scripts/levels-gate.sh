#!/usr/bin/env bash
# The gate a levels task group lands on: clippy, every workspace test, the JS runtime tests,
# the npm package check and vitest, the protocol check and every open change's strict validation.
source "$(dirname "$0")/slot.sh"
set -uo pipefail
cd "$(dirname "$0")/.."
mkdir -p target
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep -v ld64 > target/levels-clippy.log
clippy=${PIPESTATUS[0]}
tail -5 target/levels-clippy.log
cargo test --workspace --no-fail-fast -- --test-threads=4 > target/levels-test.log 2>&1
tests=$?
grep -E "^test result|FAILED|panicked" target/levels-test.log | sort | uniq -c
deno test -A crates/bevy-ts-v8/js/test 2>&1 | tail -3
deno=${PIPESTATUS[0]}
(cd packages/bevy-ts && pnpm -s check && BEVY_TS_BINARY=$PWD/../../target/debug/bevy-ts pnpm -s test 2>&1 | tail -6)
npm=$?
scripts/gen-protocol.sh --check
protocol=$?
openspec validate --changes --strict
spec=$?
echo "gate: clippy=$clippy tests=$tests deno=$deno npm=$npm protocol=$protocol spec=$spec"
[[ $clippy == 0 && $tests == 0 && $deno == 0 && $npm == 0 && $protocol == 0 && $spec == 0 ]]
