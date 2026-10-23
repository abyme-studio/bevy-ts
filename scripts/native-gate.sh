#!/usr/bin/env bash
# The gate a native task group lands on: clippy, every workspace test, the JS runtime tests,
# the npm package check and vitest, the protocol and bevy-ts/bevy checks and the change's strict validation.
source "$(dirname "$0")/slot.sh"
set -uo pipefail
cd "$(dirname "$0")/.."
mkdir -p target
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep -v ld64 > target/clippy.log
clippy=${PIPESTATUS[0]}
tail -5 target/clippy.log
cargo test --workspace --no-fail-fast -- --test-threads=4 > target/test.log 2>&1
tests=$?
grep -E "^test result|FAILED|panicked" target/test.log | sort | uniq -c
deno test -A crates/bevy-ts-v8/js/test 2>&1 | tail -3
deno=${PIPESTATUS[0]}
(cd packages/bevy-ts && pnpm -s check && BEVY_TS_BINARY=$PWD/../../target/debug/bevy-ts pnpm -s test 2>&1 | tail -6)
npm=$?
scripts/gen-protocol.sh --check
protocol=$?
scripts/gen-bevy-types.sh --check
bevy=$?
openspec validate --specs --strict
spec=$?
echo "gate: clippy=$clippy tests=$tests deno=$deno npm=$npm protocol=$protocol bevy=$bevy spec=$spec"
[[ $clippy == 0 && $tests == 0 && $deno == 0 && $npm == 0 && $protocol == 0 && $bevy == 0 && $spec == 0 ]]
