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
    Cargo.toml | Cargo.lock | rust-toolchain* | .cargo/* | scripts/* | protocol/*) full=$f; return ;;
    docs/* | README.md | .claude/* | .github/* | .gitignore | bench/*) return ;;
    .abyme/*) want scripts; return ;;
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
    [[ -n $full ]] && break
  done < <(git diff --name-only "$base"; git ls-files --others --exclude-standard)
fi

if [[ -n $full ]]; then
  crates=$(cut -f1 <<<"$members" | tr '\n' ' ')
  steps=" clippy test deno npm protocol bevy openspec scripts "
else
  # The seeds and every workspace crate that depends on one, to a fixed point.
  crates=$(jq -r --args '
    [.packages[] | {name, deps: [.dependencies[] | select(.path) | .name]}] as $p
    | def close: . as $s
        | ($s + [$p[] | select(any(.deps[]; IN($s[]))) | .name] | unique) as $n
        | if $n == $s then $s else $n | close end;
    $ARGS.positional | unique | close | join(" ")' "${seeds[@]}" <<<"$meta")
  [[ -n $crates ]] && want clippy test
  # The npm tests and bevy-ts/bevy run the engine binary, so they follow the cli.
  [[ " $crates " == *" bevy-ts-cli "* ]] && want npm bevy
fi

if [[ -n $plan ]]; then
  [[ -n $full ]] && echo "full $full"
  echo "crates $crates" | sed 's/ *$//'
  echo "steps$steps" | sed 's/ *$//'
  exit 0
fi

[[ -n $full ]] && echo "gate: full ($full)" || echo "gate: crates $crates"
if [[ -n $full ]]; then pkgs=(--workspace); else pkgs=(); for c in $crates; do pkgs+=(-p "$c"); done; fi
has() { [[ $steps == *" $1 "* ]]; }
mkdir -p target
result=""
ok=1
record() { result+=" $1=$2"; [[ $2 == 0 ]] || ok=0; }

if has clippy; then
  cargo clippy "${pkgs[@]}" --all-targets -- -D warnings 2>&1 | grep -v ld64 > target/clippy.log
  record clippy "${PIPESTATUS[0]}"
  tail -5 target/clippy.log
fi
if has test; then
  cargo test "${pkgs[@]}" --no-fail-fast -- --test-threads=4 > target/test.log 2>&1
  record tests $?
  grep -E "^test result|FAILED|panicked" target/test.log | sort | uniq -c
fi
if has deno; then
  deno test -A crates/bevy-ts-v8/js/test 2>&1 | tail -3
  record deno "${PIPESTATUS[0]}"
fi
if has npm; then
  cargo build -q -p bevy-ts-cli \
    && (cd packages/bevy-ts && pnpm install --frozen-lockfile --silent && pnpm -s check && BEVY_TS_BINARY=$PWD/../../target/debug/bevy-ts pnpm -s test 2>&1 | tail -6)
  record npm $?
fi
if has protocol; then scripts/gen-protocol.sh --check; record protocol $?; fi
if has bevy; then scripts/gen-bevy-types.sh --check; record bevy $?; fi
if has openspec; then openspec validate --specs --strict; record spec $?; fi
if has scripts; then env -u LAND_BASE deno test -A --no-check scripts/ 2>&1 | tail -3; record scripts "${PIPESTATUS[0]}"; fi

echo "gate:${result:- nothing to check}"
[[ $ok == 1 ]]
