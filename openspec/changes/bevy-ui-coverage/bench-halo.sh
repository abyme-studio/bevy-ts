#!/usr/bin/env bash
# Task 3a.5: `tsx_scale::labels_with_and_without_a_halo` in release, on this tree (arms none, 8,
# 16) and on the engine before 3a (arm none only), by turns. The engine before is a `git archive`
# of BASE (default 7669fe7e) with this tree's `tsx_scale.rs`, built in its own target folder
# (`target/halo-base`): sharing one, cargo keeps the other tree's workspace crates.
# TIP_ARMS narrows the tip's arms (`none` for 8 runs of the no-halo frame alone).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
base=${BASE:-7669fe7e}
work=$(mktemp -d /tmp/bevy-ts-halo-bench-XXXX)
build() { # $1 source tree, $2 its target folder, $3 copy of its test binary
  (cd "$1" && CARGO_TARGET_DIR="$2" cargo static test --release -p bevy-ts-cli --test it --no-run 2>&1) \
    | grep -v ld64 > "$work/build.log" || true
  sed -n 's/.*Executable tests\/it\/main.rs (\(.*\))/\1/p' "$work/build.log" | sed -n 1p > "$work/exe"
  [ -s "$work/exe" ] || { tail -30 "$work/build.log"; exit 1; }
  local exe; exe=$(cat "$work/exe"); case "$exe" in /*) ;; *) exe="$root/$exe" ;; esac
  cp "$exe" "$3"
}
mkdir -p "$work/base"
git -C "$root" archive "$base" | tar -x -C "$work/base"
cp "$root/crates/bevy-ts-cli/tests/it/tsx_scale.rs" "$work/base/crates/bevy-ts-cli/tests/it/tsx_scale.rs"
build "$work/base" "$root/target/halo-base" "$work/it-base"
build "$root" "$root/target" "$work/it-tip"
echo "built: the tree is free"
echo "base $(git -C "$root" rev-parse "$base"), tip $(git -C "$root" rev-parse HEAD) + tree, load $(uptime | sed 's/.*load averages: //')"
for r in $(seq "${BENCH_RUNS:-3}"); do
  for arm in base tip; do
    arms=none; [ "$arm" = tip ] && arms=${TIP_ARMS:-none,8,16}
    (cd "$root/crates/bevy-ts-cli" && HALO_ARMS=$arms "$work/it-$arm" tsx_scale::labels_with_and_without_a_halo --ignored --nocapture --test-threads 1 2>&1) \
      | grep -E '^\| [0-9]' | sed "s/^/$arm run $r: /"
  done
done
rm -rf "$work"
