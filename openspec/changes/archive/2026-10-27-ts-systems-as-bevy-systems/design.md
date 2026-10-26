## Context

Lanes stage 1 (`docs/research/script-threading.md`, approved 2026-10-23). Today a schedule's TS
systems run inside one exclusive system (`lib.rs` `stage_update` → `systems::run_stage`), which
takes `ScriptRuntime` and `TsSystems` out of the world (`with_systems`) and calls `run_one` for
each in order. `run_one` holds `&mut World`: it bumps the change tick, reads `SystemMemory`,
evaluates the condition, builds `ParamFrame`s from raw table columns, calls JS, stamps summary
ticks, runs the dev guard and applies the output (commands, then observers) before the next
system. Slots (`sets.rs` `ensure_slots`) are the same loop in one exclusive system per side of an
exposed set.

Bevy 0.20 facts this design rests on (`bevy_ecs-0.20.0/src`):

- `QueryState::init_access` and `QueryState::matched_tables` are public (`query/state.rs:165,198`);
  `QueryBuilder` builds a `QueryState<FilteredEntityMut>` from component ids at run time.
- Resources are components on resource entities. `Res<T>` declares a read of its component id
  `and_with(IS_RESOURCE)` through `SystemAccess::try_add` (`system/system_param.rs:685`).
  `FilteredResourcesMutParamBuilder` is deprecated in 0.20 ("Use `QueryParamBuilder` instead").
- `SystemMeta::set_non_send` / `set_has_deferred` are public. A non-send system runs on the main
  thread (`multi_threaded.rs:671`, `spawn_on_external`); `ApplyDeferred` runs on the scope thread.
- `System::set_last_run` is public. `Schedules::remove_systems_in_set` exists.
- A `Query` exposes no tables, so a built `Query<FilteredEntityMut>` cannot give the bridge whole
  column pointers.

Both neighbours have landed and this change is rebased onto them: writable-native-params
(584b78cf) and ecs-schedules stage 1 (3d641f44, protocol 1.28.0). Their leads agreed the shape below on 2026-10-23.

## Goals / Non-Goals

**Goals:**

- One Bevy system per TS system in a schedule or slot, with exactly its access.
- Today's behaviour on the main thread: same run order, same visibility of writes and spawns, same
  ticks, same one call into JS per run, a skipped system costs no call.
- A seam stage 2 fills without rework: where the call runs is one param.

**Non-Goals:**

- Running JS off the main thread, or two TS systems at once (stage 2).
- Startup, `OnEnter`/`OnExit`, observers and `call_entry` as Bevy systems. They run inside
  exclusive contexts today and keep doing so; ecs-schedules stage 2 makes states Bevy states.
- Relaxing the run-order chain between TS systems. Stage 2 decides it with lanes.
- New TS API, protocol change or dependency.

## Decisions

### 1. A TS system is a function system over one `TsParams` param

Each TS system is `IntoSystem::into_system(run_ts).with_name(identity)` built with
`(TsParamsBuilder(decl), ...)`. `TsParams` is our `unsafe impl SystemParam`:

- State: one `QueryState<FilteredEntityMut>` per TS query, built with `QueryBuilder` (`ref_id`,
  `mut_id`, `optional`, `with_id`, `without_id`, `ScriptHidden` excluded as today); the resource
  ids and write flags; the exported params' kept `FilteredAccessSet` (writable-native-params).
- `init_access`: `state.init_access(...)` for each query (the code `Query` runs); for each resource
  the `FilteredAccess` `Res`/`ResMut` build (`add_read`/`add_write` of the id, `and_with(IS_RESOURCE)`)
  through `try_add`; each exported param's access through `try_add`. A conflict inside one TS
  system fails the build naming both params, as Bevy panics for a Rust one, but as a load error.
  Bevy's own access code, so there is one owner for access.
- `get_param`: updates each query's archetypes with the cell and returns the `UnsafeWorldCell`,
  the queries, and `SystemChangeTick`. The bridge reads only the columns and rows its state
  declared, which is the safety argument for the one `unsafe`.

Rejected: `Vec<Query<FilteredEntityMut>>` from `QueryParamBuilder` plus our own `QueryState<Entity>`
for tables. Two matchers per query, and the access owner and the column walker differ.
Rejected: `FilteredResourcesMutParamBuilder`, deprecated in 0.20.

### 2. The runtime param is the lane seam

Stage 1: the system also takes `NonSendMut<ScriptRuntime>`, which marks it non-send (main thread)
and makes all TS systems conflict with each other. Stage 2 swaps exactly this param for a lane
handle (`Res<Lanes>` + the lane id) that sends the call to the lane's thread and waits; the
system becomes `Send`. Nothing else in the system changes. Other shared state the call touches
(`HostParams`, `TsPayloads`, `ScriptFrame`, `Time`) is declared as ordinary `Res`/`ResMut` params.
`HostParams` stays one `ResMut` shared by port-using systems: one `SystemState` per exported param
is sound only while TS runs serially, and stage 2 must make it per system (flag from
lead@writable-native-params).

### 3. One fetch-and-call function, two callers

`run_one` splits into `fetch(cell, ticks, sys, scratch) -> frames` and `call(rt, sys, frames)`.
The Bevy system calls them with its cell and Bevy's ticks and puts the output in its buffer. The
exclusive paths (startup, transitions, observers, `call_entry`) build the cell from `&mut World`
(`as_unsafe_world_cell`), bump the tick as today and apply the output at once. One fetch owner.

### 4. Ticks are Bevy's

`this_run`/`last_run` come from the system's `SystemChangeTick`; no `increment_change_tick` per
call. `SystemMemory` keeps `last_run` by identity; a rebuilt system gets it with `set_last_run`
before its first run, so `Changed<T>` after a reload means "since this identity last ran", as
today. writable-native-params' `RunTicks` are these.

### 5. Output goes to the system's deferred buffer

`TsParams` holds a `TsOutput` buffer (the arena bytes and `RunOutput`) and implements `apply`:
`rt.apply_output`, then each exported param's deferred `apply` in param order (the order
lead@bevy-ts ruled for writable-native-params), then `drain_observers`. `apply` takes the runtime
from the world, as `with_systems` does today; `ApplyDeferred` runs on the main thread.
`set_has_deferred` is called only when the declaration has `Commands`, `MessageWriter`,
`NextState`, or an exported param with deferred state. Summary ticks, write marks and reader
cursors are set in the system body, before it returns, with the cell.

The stage's TS systems are chained, so Bevy's auto sync inserts an `ApplyDeferred` after each
deferring system and the next TS system sees its spawns, as today. Errors and dev-guard warnings
go to the per-system stats and a log queue `pump` drains into `ScriptLog` in `First`, so a
non-deferring system needs no world access to report.

### 6. Placement and ordering

`order()` stays and yields each group's run order. A stage's systems go into `BevyTsSet::<Stage>`
and a slot's into its `BevyTsSlot` set, with `.after(previous)` between consecutive ones (a chain
keyed by system sets `TsSystemSet(identity)`, so edges survive a neighbour's rebuild). Edit mode's
`stage_should_run` becomes a `run_if` on `BevyTsSet` and the slot sets. The four `stage_*`
exclusive systems and the slot exclusive systems go; `MadeSlots` keeps the slot sets' ordering
config. `BevyTsSet` keeps its meaning for Rust: ordered after `BevyTsSet::Update` runs after every
TS `Update` system.

### 7. Run conditions are Bevy run conditions

A compiled `Condition` (`conditions.rs`) becomes a Bevy condition system (`ReadOnlySystem<In = (), Out = bool>`)
with its own declared access (`Res<TsPayloads>`, resource existence), added as the TS system's
`run_if`. ecs-schedules' native seam already yields this kind of system; its collective `runIf` is Bevy's `run_if` on the group's set, `distributiveRunIf` is
`distributive_run_if`, and its stage-3 predicate stays one evaluation per group. Each condition's
state lives in the condition system; on a rebuild it is carried by `(identity, path in the
condition tree)` from the old system to the new one, so `runOnce`, `resourceChanged`'s tick and
`onTimer`'s time survive reload (lead@ecs-schedules). This replaces the stage runner's per-stage
group cache, which could not span a group whose members sit in a stage and a slot, and the shared
`ConditionState` table: each config's condition system owns its state. Both were left open in
ecs-schedules' review.

### 8. Commit edits schedules only for what changed

`generation::apply_pending` diffs the new systems against the live ones by identity. A system
whose declaration (location ignored), placement and predecessor are unchanged keeps its Bevy
system; its `ScriptKey` is updated through the shared handle it reads at call time
(`Arc<TsSystemCell>` in `TsSystems`). Others are removed with `remove_systems_in_set(TsSystemSet(identity))`
and added fresh. Each touched schedule is edited once; Bevy rebuilds its graph on its next run.
`SystemMemory` entries (locals, cursors, `last_run`) stay keyed by identity, as today.

### 9. `bevy-ts/systems` is unchanged

`SystemInfo` reads the shared handles; `stage` still reports the bridge stage. No protocol change.

### 10. As built: where the code differs from 1–8

- `TsBevySystem` implements `System` itself, not a function system over `TsParams`. `initialize`
  must set `last_run` from `SystemMemory`, and a function system's `initialize` overwrites it.
  Access is still Bevy's: `QueryState::init_access` per query, `Res`/`ResMut` access per resource,
  each exported param's kept access. `run_body` over an `UnsafeWorldCell` is the one fetch and call;
  the exclusive paths call it through `run_one` and apply the output at once (3).
- A Rust-triggered observer still runs at the start of the next bevy-ts stage: each stage has an
  exclusive head system, `run_if(observers_queued)`, ahead of its first TS system. It runs only when
  observers are queued; the benches (6.1) measure what it costs.
- Two params of one TS system may overlap (`Query<[Transform]>` beside `Query<[Mut<Transform>]>`,
  an exported param beside a query of what it writes), as before: the body runs serially and the
  bridge aliases one column. The system declares the union; there is no internal-conflict error.
- Every TS system is deferred, not only those with a deferring param: any system may leave output
  (a signal set, a bound field written) that the next TS system must see. So the stage's chain has
  a sync point after each TS system; the benches (6.1) measure it.
- A group set's name carries a restart count, so a restart's groups get fresh condition state
  (`runOnce` runs again) while a reload keeps it.
- The log queue is flushed into `ScriptLog` by `pump` and by a system in `Last`, after
  `BevyTsSet::Last`, so a frame's errors show in the same frame.
- A schedule edit uses `World::schedule_scope`, not a `Schedules` scope: removing systems
  re-initializes the schedule, which may insert resources.
- Known limits on access, wider than exact so the system stays sound; each says how to narrow it:
  - With `reflect`, every TS system reads every resource that exists when it initializes: an
    `assets.*` host call may read any `Assets<T>`. Narrow: declare the asset types a system's
    `assets.*` calls name, from the manifest, and read only those.
  - A system with a reflect-tier query term or resource, or a Rust `MessageReader`, writes the
    whole world, so it runs alone. Narrow: resolve the reflected type's component or resource id at
    build and declare it (read or write by the term), and the message's `Messages<T>` resource.
  - A run condition holding a TS predicate or a Rust `onMessage` is not a Bevy condition: Bevy may
    evaluate one on any thread, and JS runs on the main thread only. An exclusive gate system runs
    it just before what it gates and leaves the answer for the gated system's (or group set's)
    condition. Narrow: make the gate a non-send system with the predicate's declared params as its
    access, so it runs beside non-conflicting systems.
- The stage diagnostics are each stage's TS systems' run and apply times summed, recorded once a
  frame in `Last`: there is no longer one stage system to time.

## Risks / Trade-offs

- [A stage with N TS systems pays N `ApplyDeferred` barriers instead of one] → every TS system
  defers (10.), so none is free of one; measure (task 6) a 20-system stage against today, 3 runs
  each, and Bevy's barrier alone (a sync point after each against one at the end). If it costs more than today by a margin the
  operator rejects: fall back to `chain_ignore_deferred` plus one end-of-stage sync, which changes
  "a spawn is seen by the next system" and needs its own go.
- [Schedule graph rebuild on a declaration change] → body-only edits make no edit; measure a
  rebuild of a 200-system `Update` (task 6).
- [`unsafe impl SystemParam`] → the cell is used only for the columns and resource rows the state
  declared; a test per param kind checks a conflicting Rust system is never run beside it.
- [Neighbour rebase] → both neighbours land first; the delta for "Schedules, ordering and run
  conditions" is rewritten on ecs-schedules' archived text at rebase, and ecs-schedules'
  `ts-schedules` requirement that says "one exclusive system" is modified the same way.
- [Ambiguity reports appear for games that turn detection on] → that is the point; documented.
- [Group sets and condition state are never pruned] → `LiveSystems.groups` keeps each group set
  ever configured (an empty set with its `run_if` stays in the schedule after the group changes
  its condition or goes), and `ConditionMemory` keeps state for configs no longer loaded. Both grow
  only with edits to group conditions in one session, and a restart clears them; prune at commit
  if it shows.

Measured (task 6, release, base 0594ddd7 against this branch, 8 alternating rounds, load average
194–215 throughout): medians, µs per frame over 2000 frames, then ms for reload and the next frame.

| Arm | Before (exclusive stage) | After (Bevy systems) |
|---|---|---|
| empty `Update` | 514 | 441 |
| 20 systems, no params | 627 | 709 |
| 20 systems, `Commands` | 638 | 834 |
| 20 systems, `Query<[Mut<Vel>]>` over 1000 entities | 767 | 793 |
| 200-system reload, body only | 151 | 152 |
| 200-system reload, one declaration changed | 123 | 150 |

Every arm's range overlaps the other's: at this load no difference is resolved. Bevy's barrier
alone (20 chained main-thread `Commands` systems): a sync point after each 8.0 µs per run against
one at the end 4.1 µs (best runs), about 0.2 µs per sync point. The fallback is not needed; rerun
the stage arms on a quiet machine before trusting a per-arm difference.

Rerun at load average 20–34 (8 alternating rounds, µs per frame, median and range):

| Arm | Before | After | Change |
|---|---|---|---|
| empty `Update` | 183 (110–583) | 182 (148–415) | −1% |
| 20 systems, no params | 233 (163–441) | 266 (219–359) | +14% |
| 20 systems, `Commands` | 247 (148–351) | 287 (233–1023) | +16% |
| 20 systems, `Query<[Mut<Vel>]>` over 1000 entities | 205 (166–383) | 272 (228–1159) | +33% |

The stage costs about 1.5–3.5 µs more per TS system; Bevy's sync point is 0.2 µs of that. A
15 s `sample` of the 20-`Commands` arm on each side puts the rest in two places. First, the spawn
pool: each system with `Commands` tops the pool up to 256 ids before it runs, and its
`apply_deferred` frees them all (`with_systems` → `free_pool`), so 20 such systems allocate and
free 5,120 ids a frame where the exclusive stage did it once. That is about a third of the TS
systems' samples. Second, every TS system's `apply_deferred` takes the runtime and `TsSystems` out
of the world and puts them back, even when the run left nothing. The rest is Bevy's executor
dispatching 20 systems.

Fixed: a Bevy TS system's apply keeps the spawn pool (`Last` frees it once a frame), and returns
at once when the run left nothing. Rerun at load average 25–26, 8 alternating rounds:

| Arm | Before | After | Change |
|---|---|---|---|
| empty `Update` | 151 (108–327) | 141 (98–306) | −6% |
| 20 systems, no params | 203 (144–375) | 213 (178–337) | +5% |
| 20 systems, `Commands` | 226 (141–1136) | 212 (129–322) | −6% |
| 20 systems, `Query<[Mut<Vel>]>` over 1000 entities | 218 (146–351) | 178 (140–321) | −18% |

Every arm is within noise of today's exclusive stage: the fallback is not needed.

After the port onto main (ed6b1cf6: predicates, states, reflect-tier params) and the group-gate
fix, against main itself, both built with `cargo static test --release` (main now refuses a
release test build that links the Bevy dylib, so the runs above linked it and these do not). Load
average 33–39, 8 alternating rounds, µs per frame, median and range:

| Arm | main | This branch | Change |
|---|---|---|---|
| empty `Update` | 80 (68–88) | 78 (63–91) | −2% |
| 20 systems, no params | 103 (91–126) | 104 (93–124) | +1% |
| 20 systems, `Commands` | 106 (93–117) | 105 (92–151) | −1% |
| 20 systems, `Query<[Mut<Vel>]>` over 1000 entities | 307 (282–355) | 312 (285–446) | +2% |

Still within noise on every arm.

## Migration Plan

No migration for games. Rust plugins ordered around `BevyTsSet` keep working. A Rust system that
relied on the stage being exclusive to avoid an access conflict it never declared cannot exist:
Bevy enforces declared access. Rollback is a revert.

## Open Questions

None for the design. The operator's go covers the architecture and the performance numbers from
task 6.
