#!/usr/bin/env bash
# macos-async-pipelines: the cli's thumbnail tests <runs> times, each capped at 180 s; a run that
# hangs is sampled (target/thumb-soak-<n>.sample) before it is killed.
set -uo pipefail
cd "$(dirname "$0")/.."
bin=$(cargo test -p bevy-ts-cli --test it --no-run --message-format=json 2>/dev/null | python3 -c "import sys,json
for l in sys.stdin:
    m=json.loads(l)
    if m.get('reason')=='compiler-artifact' and m.get('executable') and m['target']['name']=='it': print(m['executable'])" | tail -1)
echo "binary $bin"
for i in $(seq "${1:-15}"); do
  "$bin" viewport_thumbnail:: > target/thumb-soak-$i.log 2>&1 &
  pid=$!; t=0
  while kill -0 $pid 2>/dev/null && [ $t -lt 180 ]; do sleep 1; t=$((t+1)); done
  if kill -0 $pid 2>/dev/null; then
    sample $pid 5 -file target/thumb-soak-$i.sample >/dev/null 2>&1; kill $pid; wait $pid 2>/dev/null
    echo "run $i: HUNG after 180 s (target/thumb-soak-$i.sample)"
  else
    wait $pid; echo "run $i: exit $? $(grep -E '^test result' target/thumb-soak-$i.log) $(grep -o 'panicked at [^ ]*' target/thumb-soak-$i.log | head -2 | tr '\n' ' ')"
  fi
done
