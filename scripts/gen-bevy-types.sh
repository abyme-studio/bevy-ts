#!/usr/bin/env bash
# Regenerates `bevy-ts/bevy` (crates/bevy-ts-v8/js/bevy.js and packages/bevy-ts/bevy.d.ts) from the
# prebuilt engine's type registry, with no window and no GPU (native task 3.5).
# With --check, fails when the regenerated files differ from what is checked in.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p target
cargo build -q -p bevy-ts-cli > target/gen-bevy-types.log 2>&1 || { cat target/gen-bevy-types.log >&2; exit 1; }
./target/debug/bevy-ts types --bevy crates/bevy-ts-v8/js/bevy.js packages/bevy-ts/bevy.d.ts 2>/dev/null
(cd packages/bevy-ts && node scripts/sync-runtime.mjs >/dev/null)
if [[ "${1:-}" == "--check" ]]; then
  git diff --exit-code -- crates/bevy-ts-v8/js/bevy.js packages/bevy-ts/bevy.d.ts packages/bevy-ts/runtime/bevy.js \
    || { echo "bevy-ts/bevy is stale or edited by hand: run scripts/gen-bevy-types.sh" >&2; exit 1; }
fi
