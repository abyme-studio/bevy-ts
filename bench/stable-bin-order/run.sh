#!/usr/bin/env bash
# Both arms of bench/stable-bin-order, release: `patch` is this checkout, `base` the checkout at
# $BASE (default /tmp/leaf-flips-base, this branch's base). One target folder: the arms differ only
# in path crates, so Bevy builds once.
#   run.sh build    builds both arms, in a check slot of its own
#   run.sh time N   N runs per arm, alternating, in a second, short slot
#   run.sh N        both
set -euo pipefail
cd "$(dirname "$0")"
base=${BASE:-/tmp/leaf-flips-base}
export CARGO_TARGET_DIR="$PWD/../../target/stable-bin-order-bench"
# No sccache: its server drops under a loaded machine's builds, and a bench builds its own target.
export RUSTC_WRAPPER=""
slot=~/dev/abyme/scripts/gate-slot

build() {
  "$slot" run bevy-ts:stable-bin-order-build -- bash -c "
    set -euo pipefail
    cargo build -q --release --manifest-path Cargo.toml
    cp \$CARGO_TARGET_DIR/release/stable-bin-order-bench \$CARGO_TARGET_DIR/patch
    cargo build -q --release --manifest-path $base/bench/stable-bin-order/Cargo.toml
    cp \$CARGO_TARGET_DIR/release/stable-bin-order-bench \$CARGO_TARGET_DIR/base"
}

time_arms() {
  "$slot" run bevy-ts:stable-bin-order-time -- bash -c "
    set -euo pipefail
    for i in \$(seq $1); do
      for arm in patch base; do
        out=\$(\$CARGO_TARGET_DIR/\$arm 2>/dev/null | tail -1)
        echo \"arm=\$arm run=\$i load=\$(sysctl -n vm.loadavg | awk '{print \$2}') \$out\"
      done
    done"
}

case ${1:-3} in
  build) build ;;
  time) time_arms "${2:-3}" ;;
  *) build && time_arms "$1" ;;
esac
