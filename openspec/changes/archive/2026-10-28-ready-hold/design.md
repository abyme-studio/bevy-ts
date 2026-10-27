## Context

`count_ready` (`crates/bevy-ts-remote/src/screenshot.rs:279-291`) runs in `First`, before
`TimeSystems`. A frame is ready when the script has started (`RuntimeInfo.generation > 0`), the
render before had no pipeline waiting, and no load is in flight. Under `FixedStep` it sets
`TimeUpdateStrategy::ManualDuration(STEP)` on a ready frame and `ZERO` otherwise. The method
`bevy-ts/screenshot` counts its own ready updates (`pipelines_idle && loads.loading == 0`, same file,
the `Progress::Waiting` arm).

Agent town's walkers start their clips on `WorldInstanceReady`, after abymed's units come over a
WebSocket. The updates before they come are ready, so time steps k times before the data exists.
k varies with load. Research measured it: 13 of 13 runs' images matched by k (`findings.md` §1).

The stall watchdog (`crates/bevy-ts-remote/src/stall.rs`) prints one `Update stopped:` line with
`eprintln!` and exits 75 under `--headless`.

## Goals / Non-Goals

**Goals:**
- A game says "not yet" until its own data is in. Then every run's first step comes at the same
  point relative to its data, at any load.
- A hold that never ends fails like any other wait, and says it is the script's.
- A stall's line survives a log filter on errors.

**Non-Goals:**
- Seeing network or JS async work from the engine (it cannot close the race).
- Agent town's `LABELS_FROM` counting and its scripts (lead@engine-core).
- Changing what counts as a stall, or the 30 s limit.
- The 1 px leaf (Open Questions).

## Decisions

**A resource of names, `ReadyHold`, in `ReadyPlugin`.** `ReadyPlugin` owns readiness, so it owns
the hold: `#[derive(Resource, Default)] pub struct ReadyHold(BTreeSet<String>)` with `hold`, `release`, `is_held` and `names` (sorted, for the error). Names, not a count: two parts of a game can hold apart, a release
cannot drop someone else's hold, and the failure can say who holds. A `BTreeSet` gives the
sorted names for free. Alternatives: a bare `bool` (no names in the error, one holder); a counter (a double
release underflows or frees another's hold).

**One rule in `count_ready` and the method.** A helper `held(world) -> Option<Vec<String>>` is read
in both: `count_ready`'s `now` gains `&& !held`, so `frames` resets and `step` is false, and the
method's ready test gains the same. `not_ready` adds `held by the script: a, b` first when held.
`shoot_when_ready`'s "the script has not started" stays for generation 0. Nothing else changes:
the clock (`Clock`) sees no progress from a hold, so a hold with nothing else pending times out
after `ReadyTimeout`.

**Startup is early enough.** TS `Startup` systems run inside the generation apply that sets
`RuntimeInfo.generation` (`crates/bevy-ts/src/generation.rs`, `apply`, `run_startups`). The next
`count_ready` is the next `First`, so it sees started and held together. No frame between them can
step. A test pins it (task 2.3).

**The TS surface copies `bevy-ts/session`.** `crates/bevy-ts-cli/src/ready.rs` has a
`ScriptReadyPlugin`, a `#[derive(SystemParam)] ReadyParam { hold: ResMut<ReadyHold> }`, and
`#[export(ts = "ReadyHold", module = "bevy-ts/ready")]` with `hold(&mut self, name: &str)`,
`release(&mut self, name: &str)` and `is_held(&self, name: &str) -> bool` (TS `isHeld`). No list
of names crosses to TS: no game needs it, and `#[getter]` is not on main. It registers `ts_module("bevy-ts/ready", …)` from
`crates/bevy-ts-cli/ts/ready.{js,d.ts}`. A game takes `ready: ReadyHold` as a param, as it takes
`session: SessionBytes`. `release` writes through `DerefMut` only when the name was held, as
`SessionParam::delete` does, so a no-op release marks nothing changed. The name is Bevy-style
(`ReadyHold`, a noun, like `Time`, `ClearColor`); the module name is the concept, `ready`.
Alternatives: a global `holdReady()` callable from a WebSocket callback (needs a new op outside a
system: host calls today run only while a system, TSX or the load runs, `crates/bevy-ts-v8/src/lib.rs`
`op_bevyts_host_json`); a `stands_for = "ResMut<ReadyHold>"` claim (no gain over a plain param name,
and it needs the inner type declared in TS).

**Hot reload and Stop.** The hold is world state. A reload keeps it, and a `Startup` that already
ran does not run again, so a game that held keeps holding until its system releases. That is what
a reload mid-wait wants. Nothing is scoped per generation.

**The watchdog: no change to what is a stall.** The research's case was a synchronous macOS
pipeline compile inside `PipelineCache::process_pipeline_queue_system`. bevy-ts engines do not do
that: `vendor/bevy_render` (`Cargo.toml` `[patch.crates-io]`, since alpha.24; agent town runs
alpha.36) drops the `target_os = "macos"` cfg, so `create_pipeline_task` spawns each compile on
`AsyncComputeTaskPool` (`vendor/bevy_render/src/render_resource/pipeline_cache.rs:1058-1072`).
Only tests set `synchronous_pipeline_compilation: true`. The research's 3.0 s "longest frame" is
`pipeline_trace`'s time between two renders' `Cleanup` (`crates/bevy-ts-cli/src/headless.rs:147`),
which counts the main world's whole update under load too, not a compile. So marking
"inside the pipeline queue" as progress would guard a path that does not run, and raising the limit
would hide the real cause. What is missing is the evidence: agent town's filter `^wrote|rror`
dropped the line. The watchdog therefore also logs the line with Bevy's `error!` from its thread
(the log subscriber is global), after the `eprintln!`. Under Bevy's log it reads
`ERROR bevy_ts_remote::stall: Update stopped: …`. The raw line stays first, so tools matching
`Update stopped:` at a line start keep working. A test app with no log prints the raw line only.

## Risks / Trade-offs

- [A game holds and forgets to release] → it fails after 60 s with no progress, naming the hold.
  Only fixed-step and shot runs are affected.
- [A hold delays every shot by however long the data takes] → intended. The wait is real time and
  costs no steps.
- [The stall line appears twice on stderr under Bevy's log] → accepted: one for line matchers, one
  for error filters.
- [The next stall still has no known cause] → the line now survives agent town's filter. The fix
  for the cause comes when a line is read.

## Migration Plan

Additive. A game that does not hold behaves as before. Lands with its code.

## Open Questions

- The 1 px leaf at (300,374), off by 8, in two of the lead's runs, not in research's 13 meaning
  runs. It may be draw order between two leaves at equal depth, set by which glTF scene spawns
  first. Not cheap to check: it needs repeated loaded runs of agent town and did not reproduce.
  Open; recheck once agent town holds for its data, since the leaf may move with the walkers' k.
- Why the lead's runs stalled (exit 75): unknown until a kept `Update stopped:` line names the part.
