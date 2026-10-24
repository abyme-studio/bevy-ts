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
  not hold a slot for as long as it runs.
- **Scripts.** Each script in `scripts/` that runs cargo sources `scripts/slot.sh` first, which
  runs it again under `gate-slot run bevy-ts:<script>`. It runs as it is when `ABYME_GATE_HELD` is
  set (a heavy process, or land's check, called it) or when there is no gate-slot (CI, another machine).
  `BEVY_TS_GATE_SLOT` points it at another gate-slot.
- **By hand.** A cargo command typed in a shell is not wrapped: run it as
  `~/dev/abyme/scripts/gate-slot run bevy-ts:<what> -- cargo ...`.

`deno test -A --no-check scripts/slot.test.ts` checks all of it: each process that runs cargo is
heavy and in bash, or wraps itself in a `bevy-ts:` slot; against a fake gate-slot, each cargo
script asks for its own slot, and `ABYME_GATE_HELD` or a missing gate-slot skips it.

## The gate

`scripts/affected-gate.sh` is the check land runs on each batch. It runs `scripts/native-gate.sh`'s
steps for only what changed since `LAND_BASE`, by default the merge base with main.

Who runs what (operator, 2026-10-23): a run whose purpose is landing belongs to land, which checks
the batch, and does not sit in the check slots' queue. While a branch is in progress, its build role
runs the tests of the crates it touched, through gate-slot. To land, it runs no gate: `abyme call land
add`, then it acts on land's event (`landed`, `conflict`, or `red` with the check's log). A team may
still run `scripts/affected-gate.sh` by hand to reproduce a red batch.

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
  gate's test. Benches (`target-bench.sh` among them), other teams' gates, the release's scripts
  (`npm-*`, `pack-npm.sh`) and tools run by hand (`prune-tests.sh`, `linux-fonts.sh`) run no gate
  step, so `slot.test.ts` alone. A script no rule names runs everything.
- **Nothing** for `docs/`, `README.md`, `.claude/`, `.github/`, `.gitignore`, `bench/` and the rest of
  `.abyme/`.
- **Everything** for `Cargo.toml`, `Cargo.lock`, `rust-toolchain*`, `.cargo/`, `protocol/`, and
  any path no rule claims. `--full` forces it: `scripts/native-gate.sh` is that.

`scripts/affected-gate.sh --plan` prints the crates and steps and runs nothing; it takes no slot.
It exits 0 when every step it ran passed, and 75 when it could not start: no cargo or jq, or a
`LAND_BASE` that is not a commit. Under land that is at once: land's check runs with
`ABYME_GATE_HELD` set, so it waits for no slot. Started by hand, the `affected-gate` process is heavy
and gets its slot first, so a bad `LAND_BASE` exits 75 only once the slot is free.
Each step's whole output is `target/affected-gate/<step>.log`. A step that fails is also kept as
`target/affected-gate/failed-<UTC time>-<step>.log`, named in the gate's last line, so a flake can
be traced after land's rerun of a red check passes; the newest 10 are kept.
`deno test -A --no-check scripts/affected-gate.test.ts` checks the rules on a fixture workspace.

`.abyme/land.ts` names it as the land plugin's batch check (`check: "affected-gate"`, the process of
`.abyme/processes.ts`): land runs it on each batch stacked on main, with `LAND_BASE` set to main
before the batch. Exit 0 lands the batch, 75 sends it back unsplit, and any other exit is red.
Its `full: { check: "native-gate" }` runs the whole gate on main's tip when `.abyme/triggers.ts`
calls `land verify` at 04:30 each day; a red tip's first red landing is found and reverted. The
`native-gate` process, like every gate in `.abyme/processes.ts`, sets `pipefail`, so its `tee` never
turns a red gate green.
