#!/usr/bin/env bash
# The release prebuilt engine's stripped size and its clean build time, for script-networking-2's
# size delta (docs/networking.md). `bench/networking/size.sh <label> [rev]` builds from nothing in
# target/size (the tree as it is, or `rev`'s, exported to target/size-src), appends a row to bench/networking/results/size-runs.md, and keeps the cargo timings
# report as target/size-timings/<label>.html (per-crate times: ring, rustls, hyper), the last run's.
set -euo pipefail
label=${1:?label}
rev=${2:-}
cd "$(dirname "$0")/../.."
dir=$PWD/target/size
rm -rf "$dir" target/size-src
src=$PWD
if [ -n "$rev" ]; then
  src=$PWD/target/size-src
  mkdir -p "$src"
  git archive "$rev" | tar -x -C "$src"
fi
start=$(date +%s)
(cd "$src" && CARGO_TARGET_DIR=$dir cargo static build --release -p bevy-ts-cli --timings)
secs=$(( $(date +%s) - start ))
cp "$dir/release/bevy-ts" "$dir/bevy-ts-stripped"
strip "$dir/bevy-ts-stripped"
bytes=$(stat -f %z "$dir/bevy-ts-stripped" 2>/dev/null || stat -c %s "$dir/bevy-ts-stripped")
mkdir -p target/size-timings
cp "$dir/cargo-timings/cargo-timing.html" "target/size-timings/$label.html"
# A build of the tree as it is says so: HEAD and its uncommitted changes.
if [ -n "$rev" ]; then built=$(git rev-parse --short "$rev"); else built="$(git rev-parse --short HEAD) + tree"; fi
row="| $label | $built | $bytes | $secs |"
echo "$row" | tee -a bench/networking/results/size-runs.md
rm -rf "$dir" target/size-src
