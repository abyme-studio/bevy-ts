## Context

stop-incremental's Stop is: `levels::plan_stop` (scan, file hashes), `OnExit(Play)`, `enter_edit`, `generation::rebuild`, `restore_open`, `levels::restore` (`apply_at` respawn, stamps), `settle`, clocks, selection. At 20,000 entities (third round, load 18 to 35, medians of 3):

| arm | Stop, ms | plan | rebuild | restore |
|---|---:|---:|---:|---:|
| none touched | 98 | 7.1 | 113 | 3.5 |
| 100 touched | 131 | 7.6 | 98 | 10 |
| all touched | 207 | 4.1 | 87 | 142 |

(The parts are medians of separately timed runs, so they need not add up to the Stop.)

`generation::rebuild` is `apply_set(live, restart: true)` (`crates/bevy-ts/src/generation.rs:160`):

1. reset: despawn `TsSpawned`, forget startups, despawn TS resource entities, reset memory and payloads, run the restart handlers (tsx keeps the plan's roots; levels leaves them clean);
2. engine: `rt.backend.restart()`, which drops the deno `JsRuntime` and boots a new one (`bevy-ts-v8/src/lib.rs:659`);
3. load: `backend.load` evaluates the module set and returns the manifest; declarations checked;
4. sections: payloads, components prepared, systems built, each plugin's section check. levels' `check_section` runs `compose::prepare` on every open level (`bevy-ts-levels/src/engine.rs:351`);
5. apply: section appliers (tsx reconciles generation scenes; levels' `after_generation`), logs;
6. `run_startups`: nothing in edit, `started` is false.

Known costs from `docs/research/stop-incremental.md` (arm A, 10,000 instances): engine about 9 ms, load and apply 39 ms, of which the level prepare is about 30 (the same prepare the settle measured at 31 to 33 ms). That leaves 40 to 60 ms of today's rebuild unexplained at 20,000. Two suspects: dropping the old runtime, whose heap grew with 10,000 expansions and the play; and the restart handlers' passes over 20,000 nodes.

The machine was at load 33 when this was written, so nothing was measured for it. Task 1 splits the rebuild and measures before any cut lands.

## Goals / Non-Goals

**Goals:**
- Every Stop arm under 100 ms at 20,000 entities, by the bench.
- The rebuild's cost named by step.
- No weaker Stop: the world after it is still the one the source describes, in a fresh JS instance.

**Non-Goals:**
- The content-keyed expansion cache (the full path; the next stage).
- A play that adds or removes components on every entity: it still respawns them all.
- Reusing an isolate across generations. The spare is a new instance, never one that ran code.

## Decisions

1. **The rebuild is named steps.** `apply_set`'s restart branch becomes `reset(world)`, `restart_engine(rt)`, then the shared `load`, `sections` and `apply` of `load_and_apply`, then `run_startups`. `rebuild` calls them in order; the bench calls them one by one, as it already does with Stop's parts. No timing code in the engine. Alternative: timers in `ReloadReport`. Rejected: that is protocol, for a bench's need.

2. **A Stop does not check the levels it keeps.** levels' `check_section` skips each level path in the `StopPlan` (present during the rebuild). Correct because:
   - the module set is the live one, re-applied (`LiveSet`);
   - the live set passed this check with those files when it was applied, or the level would not be open and composed;
   - the plan hashed each of the level's files against `LevelRoot.applied` just before, and a kept level was not applied after the baseline.
   Levels not in the plan are checked as before. `after_generation` already leaves kept roots clean, so nothing re-expands them.

3. **A spare engine, booted in a later frame.** `ScriptBackend` gains `prepare_restart(&mut self)`, which boots a runtime into a held slot, and `restart` takes the held one if present, else boots. The old runtime moves to a `retired` slot and is dropped by `prepare_restart`'s caller, a `Last` system in edit or play, one frame after the restart. The fake backend counts both. Correct because a booted runtime that ran no module is identical to one booted inside the restart: same loader, extensions and options. `JsRuntime` is `!Send`, so boot and drop stay on the main thread, in a frame of their own. The inspector is started on the taken runtime, as now. Cost: one idle runtime in memory, and a frame of about 9 ms after each restart, in edit (paused) or wherever the restart ran. A shipped game never restarts, so it holds no spare. Alternative: reuse the isolate with a fresh context. Rejected: deno's module map and op state live per runtime, and a "fresh instance" would then depend on clearing them all.

4. **Write back what can be written back.** The scan already reads each component's ticks. `restore::changed` now returns, per changed node, the changed component ids and whether the archetype differs. A node is written back when its archetype is the recorded one and every changed component is in `TsxNode.components`, or in the recomputed list (`GlobalTransform`, `InheritedVisibility`, plus `restore::derived`). Write-back is `reconcile::insert` of each changed tsx component from the kept resolved node: the same bytes a spawn inserts, initial fields included. Recomputed components are left to Bevy's propagation, which runs on the next frame, as after a spawn. Everything else respawns with `apply_at`. Per node, not per subtree: a written-back parent can have a respawned child.
   - Bevy's `insert` over a present component fires `on_replace` and `on_insert` hooks and observers for it. That is Bevy's meaning of the write. A respawn fires more.
   - Why not reset non-tsx components too: a respawn gives them their required-component constructor values, and Bevy does not expose running those constructors. An entity with one changed respawns.
   - Expected: about 1 µs per entity against about 7 µs for despawn, spawn and stamp, so all touched at 20,000 goes from about 140 ms to about 20.
   Alternative: batch the respawn (`spawn_batch`). Kept as the fallback if the measured write-back is no faster.

5. **Order of work.** Task 1 (split and measure) first. If the unexplained part is neither the engine drop nor the level check, the change stops and says where it is before building 3 and 4.

## Expected numbers at 20,000 entities

From the third round and research's quiet split. To be replaced by measurements.

| arm | today, ms | after, ms |
|---|---:|---:|
| none touched | 98 | ~40 |
| 100 touched | 131 | ~45 |
| all touched | 207 | ~60 |

Rebuild after: reset about 5, engine about 1 (taken from the slot), load about 9, sections without the level about 3, apply about 5: about 25 ms against 87 to 113. Plan 4 to 8 ms. Restore 3 to 25 ms.

## Risks / Trade-offs

- [The unexplained 40 to 60 ms is somewhere else] → task 1 finds it first; decision 5.
- [The spare boot fails] → the restart boots as today; the failure is logged once.
- [Two restarts in one frame] → the second boots its own; correct, only slower.
- [A component a game changed that the recomputed list wrongly holds] → the list is tsx's, each entry with a test showing Bevy recomputes it.
- [Insert hooks fire on write-back where a plugin expects none] → documented in `docs/native.md` with the Stop's other rules.
- [The machine's load swings every number 2×] → three runs per arm, the quietest round reported, as before.

## Migration Plan

Lands after stop-incremental (archived on this branch). No protocol or format change. Each of decisions 2 to 4 reverts alone.

## Open Questions

- Where the unexplained rebuild time goes: task 1.
