#!/usr/bin/env bash
# watcher-ready: the tests that save through `bevy-ts dev`'s watcher, with no sleeps, <copies>
# processes at once for <rounds> rounds: built once with main's watcher (before), once with this
# tree's (after).
source "$(dirname "$0")/slot.sh"
set -uo pipefail
cd "$(dirname "$0")/.."
out=target/watch-soak
mkdir -p "$out"
it() {
  cargo test -p bevy-ts-cli --test it --no-run --message-format=json 2>/dev/null | python3 -c "import sys,json
for l in sys.stdin:
    m=json.loads(l)
    if m.get('reason')=='compiler-artifact' and m.get('executable') and m['target']['name']=='it': print(m['executable'])" | tail -1
}
src=crates/bevy-ts-cli/src
cp "$(it)" "$out/after"
cp "$src/dev.rs" "$src/compile.rs" "$out/"
git show main:$src/dev.rs > "$src/dev.rs"
git show main:$src/compile.rs > "$src/compile.rs"
cp "$(it)" "$out/before"
cp "$out/dev.rs" "$out/compile.rs" "$src/"
for arm in before after; do
  echo "== $arm"
  SOAK_BIN=$out/$arm scripts/levels-watch-soak.sh "${1:-8}" "${2:-5}" packages:: reload::a_saved_file_applies_as
done
