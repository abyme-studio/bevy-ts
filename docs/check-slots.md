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

- **Processes.** `.abyme/processes.ts` wraps every entry whose command runs cargo or a script in
  `scripts/` as `gate-slot run bevy-ts:<process> -- bash -c '<cmd>'`. A long-running server
  (`cube`, `rapier`, `thrusters`, `skeleton`) wraps only its `cargo build` step, so it does not hold
  a slot for as long as it runs.
- **Scripts.** Each script in `scripts/` that runs cargo sources `scripts/slot.sh` first, which
  runs it again under `gate-slot run bevy-ts:<script>`. It runs as it is when `ABYME_GATE_HELD` is
  set (a gated process called it) or when there is no gate-slot (CI, another machine).
  `BEVY_TS_GATE_SLOT` points it at another gate-slot.
- **By hand.** A cargo command typed in a shell is not wrapped: run it as
  `~/dev/abyme/scripts/gate-slot run bevy-ts:<what> -- cargo ...`.

`deno test -A --no-check scripts/slot.test.ts` checks all of it against a fake gate-slot: each
process that runs cargo, in bash and in fish, asks for the slot named after it, each cargo script
asks for its own, and `ABYME_GATE_HELD` or a missing gate-slot skips it.

## The gate

`scripts/affected-gate.sh` is what a branch runs before it lands, and what land's queue runs on a
batch (`batch_check`). It runs `scripts/native-gate.sh`'s steps for only what changed since
`LAND_BASE`: the merge base with main by default; the queue sets it to main before the batch.

- **Crates.** A changed crate brings every workspace crate that depends on it, through any kind of
  dependency (`cargo metadata`). Clippy and `cargo test -p` run on that set. A change to `bevy-ts`
  reaches nearly every crate, from the graph alone.
- **Steps.** Each runs only when its paths changed: deno for `crates/bevy-ts-v8/js/`; npm for
  `packages/`, `examples/` or the cli; the protocol check for the generator, its outputs and the
  package's lockfile; bevy-ts/bevy for the cli; openspec for `openspec/`; the scripts' tests for
  `.abyme/`. `examples/` also brings the crates whose tests run them.
- **Nothing** for `docs/`, `README.md`, `.claude/`, `.github/`, `.gitignore` and `bench/`.
- **Everything** for `Cargo.toml`, `Cargo.lock`, `rust-toolchain*`, `.cargo/`, `scripts/`,
  `protocol/`, and any path no rule claims. `--full` forces it: `scripts/native-gate.sh` is that,
  land's daily run on main.

`scripts/affected-gate.sh --plan` prints the crates and steps and runs nothing; it takes no slot.
`deno test -A --no-check scripts/affected-gate.test.ts` checks the rules on a fixture workspace.
