#!/usr/bin/env bash
# Both arms of bench/stable-bin-order, release, in one check slot: `patch` is this checkout, `base`
# the checkout at $BASE (default /tmp/leaf-flips-base, this branch's base). $1 runs per arm,
# alternating. One target folder: the arms differ only in path crates, so Bevy builds once.
set -euo pipefail
cd "$(dirname "$0")"
runs=${1:-3}
base=${BASE:-/tmp/leaf-flips-base}
export CARGO_TARGET_DIR="$PWD/../../target/stable-bin-order-bench"
# No sccache: its server drops under a loaded machine's builds, and a bench builds its own target.
export RUSTC_WRAPPER=""
mkdir -p results
exec ~/dev/abyme/scripts/gate-slot run bevy-ts:stable-bin-order-bench -- bash -c "
  set -euo pipefail
  cargo build -q --release --manifest-path Cargo.toml || exit 1
  cp \$CARGO_TARGET_DIR/release/stable-bin-order-bench \$CARGO_TARGET_DIR/patch
  cargo build -q --release --manifest-path $base/bench/stable-bin-order/Cargo.toml || exit 1
  cp \$CARGO_TARGET_DIR/release/stable-bin-order-bench \$CARGO_TARGET_DIR/base
  for i in \$(seq $runs); do
    for arm in patch base; do
      out=\$(\$CARGO_TARGET_DIR/\$arm 2>/dev/null | tail -1) || exit 1
      echo \"arm=\$arm run=\$i load=\$(sysctl -n vm.loadavg | awk '{print \$2}') \$out\"
    done
  done"
