#!/usr/bin/env bash
# land's check on each batch (.abyme/land.ts): native-gate.sh's steps for only what
# changed since LAND_BASE (default: the merge base with main), as docs/check-slots.md lists.
# Clippy and the tests run for the changed crates and every workspace crate that depends on them.
# --full runs every step on the whole workspace (native-gate.sh). --plan prints what would run.
# Exits 0 when every step it ran passed, 75 when it could not start (no cargo or jq, a bad LAND_BASE).
# What would stop it is found before it waits for a slot.
cannot() { echo "affected-gate: cannot start: $*" >&2; exit 75; }
command -v jq >/dev/null || cannot "no jq"
command -v cargo >/dev/null || cannot "no cargo"
[[ -z "${LAND_BASE:-}" ]] || git -C "$(dirname "$0")/.." rev-parse -q --verify "$LAND_BASE^{commit}" >/dev/null \
  || cannot "LAND_BASE $LAND_BASE is not a commit"
[[ " $* " == *" --plan "* ]] || source "$(dirname "$0")/slot.sh"
set -uo pipefail
cd "$(dirname "$0")/.."
plan=""; full=""
for a in "$@"; do case $a in --plan) plan=1 ;; --full) full=--full ;; esac; done

# name<TAB>dir of each workspace crate, and its workspace dependencies (any kind) as name<TAB>dep,...
meta=$(cargo metadata --no-deps --format-version 1 --offline) || cannot "cargo metadata failed"
members=$(jq -r --arg root "$(pwd -P)/" \
  '.packages[] | "\(.name)\t\(.manifest_path | ltrimstr($root) | rtrimstr("/Cargo.toml"))"' <<<"$meta")

seeds=()
steps=" "
want() { for s in "$@"; do [[ $steps == *" $s "* ]] || steps+="$s "; done; }

# What one changed path claims: crates to test, steps to run, nothing, or the full run.
claim() {
  local f=$1 name dir
  case $f in
    Cargo.toml | Cargo.lock | rust-toolchain* | .cargo/* | protocol/*) full=$f; return ;;
    # A script runs its own step, and slot.test.ts, which checks every script takes its slot. Each
    # deno test runs only for what it tests: they take minutes. A script no rule names runs everything.
    scripts/affected-gate.sh | scripts/native-gate.sh) want affected slot; return ;;
    scripts/affected-gate.test.ts) want affected; return ;;
    scripts/slot.sh | scripts/slot.test.ts | .abyme/processes.ts) want slot; return ;;
    scripts/gen-protocol.sh) want protocol slot; return ;;
    scripts/gen-bevy-types.sh) want bevy slot; return ;;
    # Benches, other teams' gates, the release's scripts and tools run by hand (pruning target/, the
    # Linux font run in docker): no gate step runs them.
    scripts/animation-bench.sh | scripts/preview-bench.sh | scripts/sccache-bench.sh | scripts/target-bench.sh | scripts/levels-gate.sh \
      | scripts/systems-gate.sh | scripts/viewport-gate.sh | scripts/npm-await.sh | scripts/npm-lib.sh \
      | scripts/npm-publish.sh | scripts/pack-npm.sh | scripts/prune-tests.sh | scripts/linux-fonts.sh) want slot; return ;;
    scripts/*.sh) want slot; full=$f; return ;;
    docs/* | README.md | .claude/* | .github/* | .gitignore | bench/* | .abyme/*) return ;;
    openspec/*) want openspec; return ;;
    examples/*) seeds+=(bevy-ts-cli bevy-ts-animation bevy-ts-levels bevy-ts-viewport); want npm; return ;;
  esac
  case $f in crates/bevy-ts-v8/js/*) want deno ;; esac
  case $f in
    crates/bevy-ts-protocol-gen/* | crates/bevy-ts-remote/src/protocol.rs | packages/bevy-ts/protocol.d.ts \
      | packages/bevy-ts/scripts/gen-protocol.mjs | packages/bevy-ts/package.json | packages/bevy-ts/pnpm-lock.yaml)
      want protocol ;;
  esac
  if [[ $f == packages/* ]]; then seeds+=(bevy-ts-cli); want npm; return; fi
  while IFS=$'\t' read -r name dir; do
    [[ $f == "$dir"/* ]] && { seeds+=("$name"); return; }
  done <<<"$members"
  full=$f
}

if [[ -z $full ]]; then
  base=${LAND_BASE:-$(git merge-base main HEAD)} || cannot "no merge base with main"
  while IFS= read -r f; do
    [[ -n $f ]] && claim "$f"
  done < <(git diff --name-only "$base"; git ls-files --others --exclude-standard)
fi

if [[ -n $full ]]; then
  crates=$(cut -f1 <<<"$members" | tr '\n' ' ')
  [[ $full == --full ]] && want slot affected
  for s in clippy alone test deno npm protocol bevy openspec; do want $s; done
else
  # The seeds and every workspace crate that depends on one, to a fixed point.
  crates=$(jq -r --args '
    [.packages[] | {name, deps: [.dependencies[] | select(.path) | .name]}] as $p
    | def close: . as $s
        | ($s + [$p[] | select(any(.deps[]; IN($s[]))) | .name] | unique) as $n
        | if $n == $s then $s else $n | close end;
    $ARGS.positional | unique | close | join(" ")' "${seeds[@]}" <<<"$meta")
  [[ -n $crates ]] && want clippy alone test
  # The npm tests and bevy-ts/bevy run the engine binary, so they follow the cli.
  [[ " $crates " == *" bevy-ts-cli "* ]] && want npm bevy
fi

if [[ -n $plan ]]; then
  [[ -n $full ]] && echo "full $full"
  echo "crates $crates" | sed 's/ *$//'
  ordered=""
  for s in clippy alone test deno npm protocol bevy openspec slot affected; do [[ $steps == *" $s "* ]] && ordered+=" $s"; done
  echo "steps$ordered"
  exit 0
fi

[[ -n $full ]] && echo "gate: full ($full)" || echo "gate: crates $crates"
if [[ -n $full ]]; then pkgs=(--workspace); else pkgs=(); for c in $crates; do pkgs+=(-p "$c"); done; fi
has() { [[ $steps == *" $1 "* ]]; }
# Each step's whole output goes to target/affected-gate/<step>.log, and a failed one is kept as
# failed-<UTC time>-<step>.log, which the next run (land's rerun of a red check) does not overwrite,
# so a flake can be named after the rerun passed. The newest 10 are kept.
logs=target/affected-gate
mkdir -p "$logs"
result=""
kept=""
ok=1
# step <name> <how to show its log> <command...>: runs it into its log, shows the lines that matter.
step() {
  local name=$1 show=$2 code f
  shift 2
  "$@" > "$logs/$name.log" 2>&1
  code=$?
  $show < "$logs/$name.log"
  result+=" $name=$code"
  if [[ $code != 0 ]]; then
    ok=0
    f="$logs/failed-$(date -u +%Y%m%dT%H%M%SZ)-$name.log"
    cp "$logs/$name.log" "$f"
    kept+=" $f"
  fi
}
tail5() { grep -v ld64 | tail -5; }
tail6() { grep -v ld64 | tail -6; }
summary() { grep -E "^test result|FAILED|panicked" | sort | uniq -c; }
# Each crate built by itself: a workspace build unifies Bevy's features across crates, so a crate
# that uses a feature only a neighbour enables builds there and fails alone, as a project engine
# building it fails.
alone_step() {
  local c
  for c in $crates; do
    echo "alone: $c"
    cargo build -q -p "$c" || return 1
  done
}
npm_step() {
  cargo build -q -p bevy-ts-cli && cd packages/bevy-ts && pnpm install --frozen-lockfile --silent \
    && pnpm -s check && BEVY_TS_BINARY=$PWD/../../target/debug/bevy-ts pnpm -s test
}

has clippy && step clippy tail5 cargo clippy "${pkgs[@]}" --all-targets -- -D warnings
has alone && step alone tail6 bash -c "crates='$crates'; $(declare -f alone_step); alone_step"
has test && step tests summary cargo test "${pkgs[@]}" --no-fail-fast -- --test-threads=4
has deno && step deno tail6 deno test -A crates/bevy-ts-v8/js/test
has npm && step npm tail6 bash -c "$(declare -f npm_step); npm_step"
has protocol && step protocol tail6 scripts/gen-protocol.sh --check
has bevy && step bevy tail6 scripts/gen-bevy-types.sh --check
has openspec && step spec tail6 openspec validate --specs --strict
has slot && step slot tail6 env -u LAND_BASE deno test -A --no-check scripts/slot.test.ts
has affected && step affected tail6 env -u LAND_BASE deno test -A --no-check scripts/affected-gate.test.ts
# shellcheck disable=SC2012
ls -t "$logs"/failed-*.log 2>/dev/null | tail -n +11 | xargs rm -f

[[ -n $kept ]] && echo "gate: failed logs kept:$kept"
echo "gate:${result:- nothing to check}"
[[ $ok == 1 ]]
