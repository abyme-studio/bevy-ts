#!/usr/bin/env bash
# Task 3b.3: agent town's label bench (heimdall-labels at 75154d9), arm `ui` at BENCH_N=1000, with
# this branch's release engine. `before` places the plates by `style` `left`/`top`, as committed;
# `after` keeps a static `style` and places them by `transform`. Agent town is read, never changed:
# each arm is a `git archive` copy in a temp folder, sharing agent town's node_modules.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
town=${AGENT_TOWN:-$HOME/dev/agent-town}
# The main checkout has no node_modules; the heimdall-labels worktree has.
modules=${AGENT_TOWN_MODULES:-$HOME/.local/share/abyme/worktrees/agent-town/heimdall-labels/node_modules}
rev=75154d9
cargo build --release -p bevy-ts-cli
bin="$root/target/release/bevy-ts"
work=$(mktemp -d /tmp/bevy-ts-town-bench-XXXX)
for arm in before after; do
  mkdir -p "$work/$arm"
  git -C "$town" archive "$rev" | tar -x -C "$work/$arm"
  ln -s "$modules" "$work/$arm/node_modules"
done
python3 - "$work/after/openspec/research/heimdall-labels/bench/game.tsx" <<'EOF'
import sys
p = sys.argv[1]
s = open(p).read()
old = """          style={() => {
            shown();
            const q = at.get(p.id) ?? p;
            return { position: "absolute", left: q.x, top: q.y, width: q.w, height: q.h, justifyContent: "center", alignItems: "center" };
          }}
"""
new = """          style={{ position: "absolute", left: 0, top: 0, width: p.w, height: p.h, justifyContent: "center", alignItems: "center" }}
          transform={() => {
            shown();
            const q = at.get(p.id) ?? p;
            return { translation: [q.x, q.y] };
          }}
"""
assert s.count(old) == 1, "the ui arm's style binding moved"
open(p, "w").write(s.replace(old, new))
EOF
echo "agent town $(git -C "$town" rev-parse "$rev"), engine $(git -C "$root" rev-parse HEAD) + tree, load $(uptime | sed 's/.*load averages: //')"
# The arms by turns, one run each, so the machine's load falls on both alike.
for r in $(seq "${BENCH_RUNS:-3}"); do
  for arm in before after; do
    (cd "$work/$arm" && BEVY_TS_BINARY="$bin" BENCH_ARMS=ui BENCH_N=1000 BENCH_RUNS=1 node openspec/research/heimdall-labels/bench.mjs) | grep '^run ' | sed "s/^run 1/$arm run $r:/"
  done
done
rm -rf "$work"
