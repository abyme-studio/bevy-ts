#!/usr/bin/env bash
# The tests of what a branch changed, and nothing else (docs/check-slots.md, "Scoped tests"): the
# crates whose files it touches, against the merge base with origin/main, uncommitted and untracked
# files included. No dependents, no workspace run.
#   CRATES  the crates, over the ones the diff names ("bevy-ts-cli bevy-ts-tsx")
#   TESTS   filters for the crates' tests/it binaries ("relationships:: saves::"): then each crate's
#           unit tests run whole and its it binary only those
#   LINT=0  no clippy
#   --plan  print the crates and steps, run nothing
[[ " $* " == *" --plan "* ]] || source "$(dirname "$0")/slot.sh"
set -uo pipefail
cd "$(dirname "$0")/.." || exit 75

crates=${CRATES:-}
npm=""
if [[ -z $crates ]]; then
  base=$(git merge-base origin/main HEAD) || { echo "scoped-test: no merge base with origin/main" >&2; exit 75; }
  while IFS= read -r f; do
    case $f in
      crates/*/*) dir=${f#crates/}; dir=crates/${dir%%/*}
        name=$(sed -n 's/^name *= *"\(.*\)"/\1/p' "$dir/Cargo.toml" 2>/dev/null | head -1)
        [[ -n $name && " $crates " != *" $name "* ]] && crates+=" $name" ;;
      examples/*) [[ " $crates " == *" bevy-ts-cli "* ]] || crates+=" bevy-ts-cli" ;;
      packages/*) npm=1 ;;
    esac
  done < <(git diff --name-only "$base"; git ls-files --others --exclude-standard)
fi
crates=$(xargs <<<"$crates")
lint=${LINT:-1}

echo "scoped-test: crates: ${crates:-none}; tests: ${TESTS:-all}; lint: $lint; npm check: ${npm:-0}"
[[ " $* " == *" --plan "* ]] && exit 0
if [[ -z $crates && -z $npm ]]; then echo "scoped-test: nothing to test (set CRATES)"; exit 0; fi

ps=()
for c in $crates; do ps+=(-p "$c"); done
status=0
if ((${#ps[@]})); then
  if [[ -n ${TESTS:-} ]]; then
    cargo test "${ps[@]}" --lib --no-fail-fast || status=1
    its=()
    for c in $crates; do
      dir=$(grep -l "^name *= *\"$c\"" crates/*/Cargo.toml | head -1); dir=${dir%/Cargo.toml}
      [[ -d $dir/tests/it ]] && its+=(-p "$c")
    done
    # shellcheck disable=SC2086 # TESTS is a list of filters
    ((${#its[@]})) && { cargo test "${its[@]}" --no-fail-fast --test it -- $TESTS || status=1; }
  else
    cargo test "${ps[@]}" --no-fail-fast || status=1
  fi
  [[ $lint == 0 ]] || cargo clippy "${ps[@]}" --all-targets -- -D warnings || status=1
fi
[[ -z $npm ]] || (cd packages/bevy-ts && pnpm -s check) || status=1
echo "scoped-test: $([[ $status == 0 ]] && echo passed || echo failed)"
exit $status
