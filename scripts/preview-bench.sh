#!/usr/bin/env bash
# Change asset-preview, task 4.2: the game's frame time, memory and CPU in each scenario, one
# release engine per scenario, rows appended to target/preview-bench.md.
# PREVIEW_MODEL=/path/model.glb scripts/preview-bench.sh [scenario ...]
source "$(dirname "$0")/slot.sh"
set -uo pipefail
cd "$(dirname "$0")/.."
: "${PREVIEW_MODEL:?a .glb to preview}"
scenarios=("$@")
[[ ${#scenarios[@]} -eq 0 ]] && scenarios=(none visible hidden idle idle-edit)
cargo test --release -p bevy-ts-cli --test preview_bench --no-run 2>&1 | grep -E "^error" -A 5
for s in "${scenarios[@]}"; do
  SCENARIO=$s cargo test --release -p bevy-ts-cli --test preview_bench -- --ignored --nocapture 2>&1 | grep -E '^\||panicked|assert'
done
