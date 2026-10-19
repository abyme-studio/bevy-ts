#!/usr/bin/env bash
# Regenerates the protocol types from protocol/bevy-ts.openrpc.json: Rust (typify) into
# crates/bevy-ts-remote/src/protocol.rs and TS into packages/bevy-ts/protocol.d.ts.
# With --check, fails when the regenerated files differ from what is checked in.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run -q -p bevy-ts-protocol-gen
(cd packages/bevy-ts && pnpm install --frozen-lockfile --silent && node scripts/gen-protocol.mjs)
if [[ "${1:-}" == "--check" ]]; then
  git diff --exit-code -- crates/bevy-ts-remote/src/protocol.rs packages/bevy-ts/protocol.d.ts \
    || { echo "protocol types are stale or edited by hand: run scripts/gen-protocol.sh" >&2; exit 1; }
fi
