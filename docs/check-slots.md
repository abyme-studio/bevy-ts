# Check slots

On the development Mac, every cargo build, test, lint and bench in this repo runs inside one of the
machine's two check slots, shared with every other repo and worktree there. Parallel bevy-ts builds
pushed the Mac to a load near 40 and the operator could not log in (2026-10-23); two at a time
finish, and the rest wait their turn.

The slots belong to abyme's `~/dev/abyme/scripts/gate-slot`:

```sh
~/dev/abyme/scripts/gate-slot run bevy-ts:<name> -- <cmd>
```

holds a slot while `<cmd>` runs and passes `ABYME_GATE_HELD` to it, so a step inside takes none of
its own. A run that finds both slots held prints who holds them and waits.

The repo takes the slot itself, so nothing has to remember to:

- **Processes.** `.abyme/processes.ts` marks every entry whose command runs cargo or a script in
  `scripts/` `heavy: true`, its command as `bash -c '<cmd>'` (the operator's fish has no
  `pipefail`). abyme holds a check slot while a heavy process runs, through gate-slot, so what it
  runs takes none of its own. An entry that wraps itself in gate-slot is left alone: a long-running
  server (`cube`, `rapier`, `thrusters`, `skeleton`) wraps only its `cargo build` step, so it does
  not hold a slot for as long as it runs. Every other run on a slot lives 30 minutes at most
  (`limit`); an entry that needs longer sets its own and says why (the cold-build benches).
- **Scripts.** Each script in `scripts/` that runs cargo sources `scripts/slot.sh` first, which
  runs it again under `gate-slot run bevy-ts:<script>`. It runs as it is when `ABYME_GATE_HELD` is
  set (a heavy process, or land's check, called it) or when there is no gate-slot (CI, another machine).
  `BEVY_TS_GATE_SLOT` points it at another gate-slot.
- **By hand.** A cargo command typed in a shell is not wrapped: run it as
  `~/dev/abyme/scripts/gate-slot run bevy-ts:<what> -- cargo ...`.

`deno test -A --no-check scripts/slot.test.ts` checks all of it: each process that runs cargo is
heavy and in bash, or wraps itself in a `bevy-ts:` slot; against a fake gate-slot, each cargo
script asks for its own slot, and `ABYME_GATE_HELD` or a missing gate-slot skips it.

## Scoped tests

The rule (operator, 2026-10-28): a branch runs the tests of what it changed, and then lands.

- Run `scoped`: `abyme call processes start name=scoped`. It runs `scripts/scoped-test.sh`: `cargo
  test` and clippy on the crates whose files the branch changed since its merge base with
  origin/main, uncommitted and untracked files included, and `pnpm check` when `packages/` changed.
  No dependents and no workspace run. Its env, for one run (`env:='{...}'`): `CRATES` names the
  crates instead, `TESTS` runs each crate's unit tests and only those filters of its `tests/it`
  (`"relationships:: saves::"`), `LINT=0` skips clippy. `scripts/scoped-test.sh --plan` prints
  what it would run. Its log is `target/scoped.log`.
- `bevy-ts-cli`'s whole `tests/it` is long: a change there names `TESTS`.
- Generated files have their own entries: `gen-bevy-types`, `gen-protocol`. `npm-test` runs the
  package's vitest suite.
- No per-team gate entry in `.abyme/processes.ts`: pass `env` to `scoped`. No full-workspace run,
  load-flake sweep or hour-long gate before landing. `.abyme/land.ts` checks nothing.
- Never rerun a failed run unchanged. Read its log, change something, then run it again.
- A red main is fixed forward, by whoever's change it names, never reverted.

## Reproducing a red batch

`scripts/affected-gate.sh` (the `affected-gate` process) runs `scripts/native-gate.sh`'s steps for
only what changed since `LAND_BASE`, by default the merge base with main. Run it by hand only to
reproduce a failure on main; it is wider than `scoped`.

- **Crates.** A changed crate brings every workspace crate that depends on it, through any kind of
  dependency (`cargo metadata`). Clippy and `cargo test -p` run on that set. A crate's integration
  tests are one binary (`tests/it`, docs/build-cache.md), so the crate is the unit: the gate selects
  no test files. A change to `bevy-ts` reaches nearly every crate, from the graph alone.
- **Alone.** Each changed crate is built by itself (`cargo build -p <crate>`, one at a time). A
  workspace build unifies Bevy's features across crates, so a crate that uses a feature only a
  neighbour turns on builds there and fails alone, as it fails in a project engine. When a crate's
  `Cargo.toml` changed, every crate the change reaches is built alone, since its features moved.
- **Steps.** Each runs only when its paths changed: deno for `crates/bevy-ts-v8/js/`; npm for
  `packages/`, `examples/` or the cli; the protocol check for the generator, its outputs and the
  package's lockfile; bevy-ts/bevy for the cli; openspec for `openspec/`. The two deno tests take minutes,
  so each runs only for what it tests: `slot.test.ts` for `.abyme/processes.ts`, `slot.sh` and
  every script, `affected-gate.test.ts` for the gate and its test; both under `--full`.
  `examples/` also brings the crates whose tests run them.
- **Scripts.** A script runs its own step and `slot.test.ts`, not everything: `gen-protocol.sh` the
  protocol check, `gen-bevy-types.sh` bevy-ts/bevy, `affected-gate.sh` and `native-gate.sh` the
  gate's test. Benches (`target-bench.sh` among them), `scoped-test.sh`, the release's scripts
  (`npm-*`, `pack-npm.sh`) and tools run by hand (`prune-tests.sh`, `linux-fonts.sh`) run no gate
  step, so `slot.test.ts` alone. A script no rule names runs everything.
- **Nothing** for `docs/`, `README.md`, `.claude/`, `.github/`, `.gitignore`, `bench/`, the rest of
  `.abyme/` and `scripts/gate-stats.mjs`.
- **Everything** for `Cargo.toml`, `Cargo.lock`, `rust-toolchain*`, `.cargo/`, `protocol/`, and
  any path no rule claims. `--full` forces it: `scripts/native-gate.sh` is that.

`scripts/affected-gate.sh --plan` prints the crates and steps and runs nothing; it takes no slot.
It exits 0 when every step it ran passed, and 75 when it could not start: no cargo or jq, or a
`LAND_BASE` that is not a commit. Started as a process it is heavy and gets its slot first, so a
bad `LAND_BASE` exits 75 only once the slot is free.
Each step's whole output is `target/affected-gate/<step>.log`. A step that fails is also kept as
`target/affected-gate/failed-<UTC time>-<step>.log`, named in the gate's last line; the newest 10
are kept. `deno test -A --no-check scripts/affected-gate.test.ts` checks the rules on a fixture
workspace. The `affected-gate` process sets `pipefail`, so its `tee` never turns a red gate green.

## Slot use

`node scripts/gate-stats.mjs [hours]` reads gate-slot's history (`$ABYME_GATE/history.jsonl`,
`/tmp/abyme-heavy` by default) and prints, per repository over the last `hours` (72): its runs, how
many failed, the slot hours they held and their share of all slot hours. `--repo bevy-ts` splits one
repository by run name. On 2026-10-28 bevy-ts held 64% of 316 slot hours in 72 h, and 38% of its
runs failed.
