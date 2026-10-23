#!/bin/bash
# docs/fonts.md: the Linux engine and text_fonts test, built in Debian and run in containers with and
# without fontconfig and the Noto CJK and emoji fonts. Needs docker; the build caches in two volumes.
# Usage: scripts/linux-fonts.sh   (logs to target/linux-fonts/)
set -uo pipefail
cd "$(dirname "$0")/.."
out=target/linux-fonts
mkdir -p "$out"
vols=(-v "$PWD:/src:ro" -v bevy-ts-linux-target:/target -v bevy-ts-linux-cargo:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/target)
# The release workflow's Linux build packages, fontconfig's included.
deps="libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev libfontconfig1-dev pkg-config clang"

echo "== build"
docker run --rm "${vols[@]}" rust:1-bookworm bash -c "
  set -e
  apt-get update -qq && apt-get install -y -qq $deps >/dev/null
  cd /src
  cargo build --locked -p bevy-ts-cli --bin bevy-ts 2>&1 | grep -E '^(error|warning: unused)' || true
  cargo test --locked -p bevy-ts-cli --test text_fonts --no-run 2>&1 | grep -E '^error|Executable' || true
  mkdir -p /target/out && cp /target/debug/bevy-ts /target/out/
  cp \$(ls -t /target/debug/deps/text_fonts-* | grep -v '\.d$' | head -1) /target/out/text_fonts
  echo NEEDED; readelf -d /target/out/bevy-ts | grep NEEDED
" 2>&1 | tee "$out/build.log" || exit 1

# A runtime image with what the engine links, minus $1 (a package left out).
run() { # $1 label, $2 packages, $3 command
  echo "== $1"
  docker run --rm "${vols[@]}" debian:bookworm-slim bash -c "
    apt-get update -qq && apt-get install -y -qq --no-install-recommends $2 >/dev/null
    cd /tmp; $3; echo exit \$?
  " 2>&1 | grep -vE 'debconf|^Selecting|^Preparing|^Unpacking|^Setting up|^Processing' | tail -25 | tee "$out/$1.log"
}

# No libfontconfig1: what a player on a bare Linux sees.
run no-fontconfig "libasound2 libudev1" "/target/out/bevy-ts --help >/dev/null"
# fontconfig, and lavapipe for a headless GPU, but no CJK or emoji fonts.
run no-noto "libasound2 libudev1 libfontconfig1 fonts-dejavu-core mesa-vulkan-drivers" "/target/out/text_fonts --test-threads 1 2>&1 | grep -E 'notdef|test result|panicked'"
# Everything docs/fonts.md asks of a Linux test host.
run noto "libasound2 libudev1 libfontconfig1 fonts-dejavu-core fonts-noto-cjk fonts-noto-color-emoji mesa-vulkan-drivers" "/target/out/text_fonts --test-threads 1 --nocapture 2>&1 | grep -E 'notdef|layout with|test result|panicked'"
