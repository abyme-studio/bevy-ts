#!/usr/bin/env bash
# The gate a viewport task group lands on: clippy -D warnings, every workspace test, deno check.
set -uo pipefail
cd "$(dirname "$0")/.."
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep -v ld64 > target/clippy.log
clippy=${PIPESTATUS[0]}
tail -5 target/clippy.log
cargo test --workspace -- --test-threads=4 > target/test.log 2>&1
tests=$?
grep -E "^test result|FAILED|panicked" target/test.log | sort | uniq -c
(cd packages/bevy-ts && deno check protocol.d.ts index.d.ts test/tsx/elements.tsx)
deno=$?
echo "gate: clippy=$clippy tests=$tests deno=$deno"
[[ $clippy == 0 && $tests == 0 && $deno == 0 ]]
