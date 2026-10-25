#!/usr/bin/env bash
# macos-async-pipelines: the arena headless, time to all render pipelines ready.
#   scripts/pipeline-bench.sh prep <arm>             build the CLI and the arena into target/pipelines-<arm>/
#   scripts/pipeline-bench.sh run <arm> <runs> [cold] play it <runs> times, BEVY_TS_PIPELINE_TRACE on
# A run ends 15 s after its last "pipelines ready" line (240 s at most) and prints that line with
# the load average. `cold` runs under a sandbox that cannot read or write the user's Metal shader
# cache, so every shader compiles afresh without clearing the cache other processes share.
source "$(dirname "$0")/slot.sh"
set -uo pipefail
cd "$(dirname "$0")/.."
dir=target/pipelines-$2
case $1 in
prep)
  cargo build -p bevy-ts-cli 2>&1 | grep -v ld64 | tail -3
  rm -rf "$dir" && mkdir -p "$dir"
  cp target/debug/bevy-ts "$dir/bevy-ts"
  CARGO_TARGET_DIR=$PWD/target "$dir/bevy-ts" build examples/arena --out "$dir/dist" >"$dir/build.log" 2>&1
  echo "prep exit $?"; ls "$dir/dist"
  ;;
run)
  mode=${4:-warm}
  metal=$(cd "$(getconf DARWIN_USER_CACHE_DIR)com.apple.metal" && pwd -P)
  for i in $(seq "$3"); do
    log=$dir/$mode-$i.log
    if [ "$mode" = cold ]; then BEVY_TS_PIPELINE_TRACE=1 sandbox-exec -p "(version 1)(allow default)(deny file-read* file-write* (subpath \"$metal\"))" "$dir/bevy-ts" play "$dir/dist/game" --headless >"$log" 2>&1 &
    else BEVY_TS_PIPELINE_TRACE=1 "$dir/bevy-ts" play "$dir/dist/game" --headless >"$log" 2>&1 & fi
    pid=$!
    start=$SECONDS; seen=0; quiet=$SECONDS
    while kill -0 $pid 2>/dev/null && [ $((SECONDS - start)) -lt 240 ]; do
      n=$(grep -c '^pipelines ready' "$log")
      [ "$n" != "$seen" ] && seen=$n && quiet=$SECONDS
      [ "$n" -gt 0 ] && [ $((SECONDS - quiet)) -ge 15 ] && break
      sleep 1
    done
    kill $pid 2>/dev/null; wait $pid 2>/dev/null
    echo "$2 $mode $i: $(grep '^pipelines ready' "$log" | tail -1) errors=$(grep -c 'ERROR\|panicked' "$log") load=$(sysctl -n vm.loadavg | cut -d' ' -f2)"
  done
  ;;
esac
