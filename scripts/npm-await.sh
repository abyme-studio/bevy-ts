#!/usr/bin/env bash
# Fails unless every platform (packages/platforms/*) built, then waits until each is on npm at the
# version, so bevy-ts never goes out missing an engine. A package just published can take a few
# minutes to show in `npm view`.
#   scripts/npm-await.sh <version> <built: darwin-arm64,linux-x64,...>
set -uo pipefail
version=$1 list=$2
source "$(dirname "$0")/npm-lib.sh"

unbuilt=()
for dir in "$(dirname "$0")"/../packages/platforms/*/; do
  p=$(basename "$dir")
  [[ ",$list," == *",$p,"* ]] || unbuilt+=("$p")
done
if [ ${#unbuilt[@]} -gt 0 ]; then
  echo "::error::engine did not build: ${unbuilt[*]}; bevy-ts not published. Re-run the failed jobs." >&2
  exit 1
fi

missing=()
for p in ${list//,/ }; do
  for wait in 0 15 30 60 120; do
    sleep "$wait"
    [ "$(on_npm "@bevy-ts/$p" "$version")" = yes ] && { echo "@bevy-ts/$p@$version is on npm"; continue 2; }
  done
  missing+=("$p")
done
if [ ${#missing[@]} -gt 0 ]; then
  echo "::error::not on npm at $version: ${missing[*]}; bevy-ts not published. Re-run the failed jobs." >&2
  exit 1
fi
