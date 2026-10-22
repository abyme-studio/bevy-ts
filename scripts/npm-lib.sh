# shellcheck shell=bash
# Shared by the release scripts; sourced, not run.

# on_npm <name> <version>: prints yes, no, or error. Retries npm's transient errors; a 404 means
# the package itself is not on npm, so no.
on_npm() {
  local out delay=5
  for _ in 1 2 3 4 5; do
    if out=$(npm view "$1@$2" version 2>&1); then
      [ -n "$out" ] && echo yes || echo no
      return
    fi
    if grep -q E404 <<<"$out"; then echo no; return; fi
    sleep "$delay"; delay=$((delay * 2))
  done
  echo error
}
