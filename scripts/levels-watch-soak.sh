#!/usr/bin/env bash
# levels-watch-flake: levels_protocol's watch-order test, <copies> processes at once for <rounds>
# rounds, each its own engine and scratch folder: the load a full workspace run puts on it.
source "$(dirname "$0")/slot.sh"
set -uo pipefail
cd "$(dirname "$0")/.."
bin=$(cargo test -p bevy-ts-cli --test it --no-run --message-format=json 2>/dev/null | python3 -c "import sys,json
for l in sys.stdin:
    m=json.loads(l)
    if m.get('reason')=='compiler-artifact' and m.get('executable') and m['target']['name']=='it': print(m['executable'])" | tail -1)
echo "binary $bin"
test=levels_protocol::watch_deltas_keep_a_client_in_tsx_tree_order
out=target/levels-watch-soak/$(date +%s)
mkdir -p "$out"
fails=0
for r in $(seq "${2:-5}"); do
  for c in $(seq "${1:-8}"); do
    "$bin" --exact "$test" > $out/$r-$c.log 2>&1 &
  done
  wait
  for c in $(seq "${1:-8}"); do
    log=$out/$r-$c.log
    if grep -q '^test result: ok' "$log"; then rm "$log"; else fails=$((fails+1)); echo "round $r copy $c: FAILED ($log) $(grep -A3 'panicked at' "$log" | head -4 | cut -c1-400)"; fi
  done
done
echo "failures: $fails of $((${1:-8} * ${2:-5}))"
