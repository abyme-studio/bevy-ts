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

Neighbours land first and this change rebases onto them: ecs-schedules (stage 1 at tests) and
writable-native-params (at review). Their leads agreed the shape below on 2026-10-23.

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

A compiled `Condition` becomes a Bevy condition system (`ReadOnlySystem<In = (), Out = bool>`)
with its own declared access (`Res<TsPayloads>`, resource existence), added as the TS system's
`run_if`. After the rebase, ecs-schedules' conditions and its native seam are the same kind of
system; its collective `runIf` is Bevy's `run_if` on the group's set, `distributiveRunIf` is
`distributive_run_if`, and its stage-3 predicate stays one evaluation per group. Each condition's
state lives in the condition system; on a rebuild it is carried by `(identity, path in the
condition tree)` from the old system to the new one, so `runOnce`, `resourceChanged`'s tick and
`onTimer`'s time survive reload (lead@ecs-schedules).

### 8. Commit edits schedules only for what changed

`generation::apply_pending` diffs the new systems against the live ones by identity. A system
whose declaration (location ignored), placement and predecessor are unchanged keeps its Bevy
system; its `ScriptKey` is updated through the shared handle it reads at call time
(`Arc<TsSystemCell>` in `TsSystems`). Others are removed with `remove_systems_in_set(TsSystemSet(identity))`
and added fresh. Each touched schedule is edited once; Bevy rebuilds its graph on its next run.
`SystemMemory` entries (locals, cursors, `last_run`) stay keyed by identity, as today.

### 9. `bevy-ts/systems` is unchanged

`SystemInfo` reads the shared handles; `stage` still reports the bridge stage. No protocol change.

## Risks / Trade-offs

- [A stage with N deferring TS systems pays up to N `ApplyDeferred` barriers instead of one] →
  systems without deferred params pay none; measure (task 6) a 20-system stage, all deferring and
  none, against today, 3 runs each on an idle slot. If it costs more than today by a margin the
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

## Migration Plan

No migration for games. Rust plugins ordered around `BevyTsSet` keep working. A Rust system that
relied on the stage being exclusive to avoid an access conflict it never declared cannot exist:
Bevy enforces declared access. Rollback is a revert.

## Open Questions

None for the design. The operator's go covers the architecture and the performance numbers from
task 6.
