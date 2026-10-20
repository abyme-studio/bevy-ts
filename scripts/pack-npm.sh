#!/usr/bin/env bash
# Builds the release engine for this machine, copies it into its platform package, and packs
# both npm packages into dist/npm. Publishing is a separate, deliberate step (docs/release.md).
# Set versions and optionalDependencies first: node scripts/set-npm-version.mjs <version> <platform>.
set -euo pipefail
cd "$(dirname "$0")/.."
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) platform=darwin-arm64 ;;
  Darwin-x86_64) platform=darwin-x64 ;;
  Linux-x86_64) platform=linux-x64 ;;
  Linux-aarch64) platform=linux-arm64 ;;
  *) echo "unsupported host $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac
[[ "${1:-}" == "--no-build" ]] || cargo build --release -p bevy-ts-cli
install -m 755 target/release/bevy-ts "packages/platforms/$platform/bin/bevy-ts"
mkdir -p dist/npm
(cd "packages/platforms/$platform" && npm pack --silent --pack-destination ../../../dist/npm)
(cd packages/bevy-ts && npm pack --silent --pack-destination ../../dist/npm)
ls -1 dist/npm
