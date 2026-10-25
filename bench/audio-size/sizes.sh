#!/usr/bin/env bash
# One release build per arm; prints `arm bytes stripped-bytes`. Sizes are deterministic, so one
# build per arm is the measurement.
set -euo pipefail
cd "$(dirname "$0")"
for arm in none vorbis wav flac mp3 vorbis,wav symphonia-vorbis symphonia-wav symphonia-flac; do
  if [ "$arm" = none ]; then f=(); else f=(--features "$arm"); fi
  cargo build --release --quiet "${f[@]}"
  bin=target/release/audio-size
  cp "$bin" "target/$arm" && strip -o "target/$arm.stripped" "target/$arm"
  echo "$arm $(stat -f%z "target/$arm") $(stat -f%z "target/$arm.stripped")"
done
