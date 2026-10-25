#!/usr/bin/env bash
# Change script-gizmos task 2.11: examples/gizmos headless on main's engine without src/gizmos.ts
# (before.png), then on this tree's engine with it (after.png), into ~/dev/bevy-ts-frames/script-gizmos.
#   scripts/gizmos-frames.sh <main engine> <this engine>
set -euo pipefail
out=$HOME/dev/bevy-ts-frames/script-gizmos
mkdir -p "$out"
shot() { # <engine> <project dir> <png>
  rm -f "$3" "$3.2.png"
  BEVY_TS_SCREENSHOT=$3 NO_COLOR=1 "$1" dev "$2/src/game.tsx" --headless --remote-port 0 >"$3.log" 2>&1 &
  local pid=$!
  for _ in $(seq 600); do
    [ -f "$3.2.png" ] && break
    kill -0 $pid 2>/dev/null || break
    sleep 0.5
  done
  kill $pid 2>/dev/null || true
  wait $pid 2>/dev/null || true
  [ -f "$3" ] || { echo "no $3; log:"; tail -20 "$3.log"; exit 1; }
  if grep -q "generation [0-9]* failed" "$3.log"; then echo "$3: the script failed:"; grep "failed" "$3.log"; exit 1; fi
  echo "wrote $3"
}
before=$(mktemp -d)
cp -R examples/gizmos/src examples/gizmos/tsconfig.json "$before/"
rm "$before/src/gizmos.ts"
sed -i.bak '/import ".\/gizmos.ts";/d' "$before/src/game.tsx" && rm "$before/src/game.tsx.bak"
shot "$1" "$before" "$out/before.png"
rm -rf "$before"
shot "$2" "$PWD/examples/gizmos" "$out/after.png"
