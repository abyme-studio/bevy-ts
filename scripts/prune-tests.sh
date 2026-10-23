#!/bin/bash
# docs/build-cache.md, "Pruning old test executables": cargo never deletes a superseded executable,
# so each `cargo test -p …` selection that relinks a test leaves the last copy behind. This keeps
# the newest copy of each executable in target/*/deps (by its name and the source file it was built
# from, read from its .d file) and drops the older ones, with their .d and .dSYM. Not while a
# build or test runs in that target/: it may drop the copy that run is about to execute.
# Usage: scripts/prune-tests.sh [--dry-run] [target dir, default target]
set -euo pipefail
dry=
if [ "${1:-}" = --dry-run ]; then dry=1; shift; fi
cd "$(dirname "$0")/.."
target=${1:-target}
freed=0
n=0
for deps in "$target"/*/deps; do
  [ -d "$deps" ] || continue
  # "<root source>\t<mtime>\t<path>" for every executable without an extension.
  find "$deps" -maxdepth 1 -type f -perm -u+x ! -name '*.*' -print0 |
    while IFS= read -r -d '' exe; do
      d=$exe.d
      [ -f "$d" ] || continue
      root=$(head -1 "$d" | awk '{print $2}')
      name=$(basename "$exe")
      printf '%s %s\t%s\t%s\n' "${name%-*}" "$root" "$(stat -c %Y "$exe" 2>/dev/null || stat -f %m "$exe")" "$exe"
    done | sort -t$'\t' -k1,1 -k2,2nr >"$deps/.prune-list"
  prev=
  while IFS=$'\t' read -r key _ exe; do
    if [ "$key" = "$prev" ]; then
      size=$( (du -sk "$exe" "$exe.dSYM" 2>/dev/null || true) | awk '{s += $1} END {print s}')
      freed=$((freed + size))
      n=$((n + 1))
      if [ -z "$dry" ]; then rm -rf "$exe" "$exe.d" "$exe.dSYM"; else echo "would drop $exe"; fi
    fi
    prev=$key
  done <"$deps/.prune-list"
  rm -f "$deps/.prune-list"
done
echo "${dry:+would have }dropped $n older executables, $((freed / 1024)) MB"
