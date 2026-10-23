#!/usr/bin/env bash
# The full gate, on the whole workspace: clippy, every workspace test, the JS runtime tests, the npm
# package check and vitest, the protocol and bevy-ts/bevy checks, openspec's strict validation and the
# scripts' tests. Its steps live in affected-gate.sh, which runs them scoped.
source "$(dirname "$0")/slot.sh"
exec "$(dirname "$0")/affected-gate.sh" --full
