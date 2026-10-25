#!/bin/bash
# Change script-networking-2, task 6.2: @abyme/client's `held` mode asks `/api/plugins` through
# `fetch` while its socket rests. A test abymed on port 4593 (abyme's scripts/test-abymed, never
# the live one on 4477), the game headless under `bevy-ts dev`, both stopped after 30 s.
# Needs ~/dev/abyme (scripts/test-abymed and the client's source) and a debug `bevy-ts`.
set -u
wt=$(cd "$(dirname "$0")/../../.." && pwd)
abyme=${ABYME_REPO:-$HOME/dev/abyme}
game=$wt/target/held-game
mkdir -p "$game/src"
echo '{}' > "$game/tsconfig.json"
echo '{ "net": { "allow": ["127.0.0.1:4593"] } }' > "$game/bevy-ts.json"
connect=$(python3 -c "import os,sys;print(os.path.relpath(sys.argv[1], sys.argv[2]))" "$abyme/web/packages/client/src/connect.ts" "$game/src")
sed "s|@CONNECT@|$connect|" "$wt/bench/networking/held/main.ts.in" > "$game/src/main.ts"
# abyme's own test daemon: fixtures/config, a scratch state folder, its process host stopped and
# the folder deleted when it ends; it rebuilds a stale release abymed first, in a check slot.
ABYME_PORT=4593 "$abyme/scripts/test-abymed" > "$game/abymed.log" 2>&1 &
daemon=$!
for i in $(seq 900); do curl -sf http://127.0.0.1:4593/api/plugins >/dev/null && break; sleep 1; done
echo "abymed answered after ${i}s"
DYLD_FALLBACK_LIBRARY_PATH="$wt/target/debug/deps:$(rustc --print target-libdir)" NO_COLOR=1 "$wt/target/debug/bevy-ts" dev "$game/src/main.ts" --headless > "$game/game.log" 2>&1 &
engine=$!
sleep 30
kill $engine $daemon; wait
grep -E "held-|rror|refused|not in" "$game/game.log" | head -30
