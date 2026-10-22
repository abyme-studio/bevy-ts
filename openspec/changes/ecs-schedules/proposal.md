## Why

The coverage audit (`docs/research/bevy-coverage.md`, ECS rows) found that a TS script reaches four of Bevy's schedules, five run conditions, no one-shot systems, and state machines that are not Bevy states: no `PreviousState`, no `OnTransition`, no `DespawnOnExit`, no `SubStates` or `ComputedStates`. Rust sees none of a TS state. Research (`docs/research/ecs-schedules.md`) splits the fix into three stages; this change specs all three and lands stage 1 first.

## What Changes

Stage 1 (follows existing patterns):

- **Every main and fixed schedule.** `First`, `PreUpdate`, `RunFixedMainLoop`, `Update`, `SpawnScene`, `PostUpdate`, `Last`, `FixedFirst`, `FixedPreUpdate`, `FixedUpdate`, `FixedPostUpdate`, `FixedLast`, plus `PreStartup` and `PostStartup` around TS `Startup`. A new schedule's bevy-ts stage is made the first time a generation names it, as slots are, so an unused schedule costs nothing. The four existing stages stay as they are.
- **Custom schedules.** `class Waves extends ScheduleLabel {}`, `app.insertScheduleAfter/Before(Update, Waves)` (and the `Fixed` and startup forms), `app.initSchedule(Waves)` and `commands.runSchedule(Waves)`. A Rust crate exposes its own schedule to TS with `app.ts_schedule("port:name", Label)`.
- **Rust-evaluated conditions with Bevy 0.20 names.** `runOnce`, `resourceAdded`, `resourceChanged`, `resourceRemoved`, `resourceChangedOrRemoved`, `resourceExistsAndChanged`, `resourceEquals`, `resourceExistsAndEquals`, `anyWithComponent`, `anyComponentRemoved`, `anyMatchFilter`, `conditionChanged`, `conditionChangedTo`, and the combinators `andThen`, `orElse`, `nandThen`, `norElse`, `xor`, `xnor` and the `Eager` forms. Each stateful condition keeps its own tick or cursor.
- **The condition seam.** `app.ts_condition("crate:name", factory)` exposes a Rust condition system to TS by name, as `ts_set` exposes a set. TS calls it with `nativeCondition(name, args)`; ports wrap that in a typed function. script-time (`onTimer`) and script-input-devices (`inputJustPressed`) plug in here: they own their conditions, this change owns the machinery.
- **One-shot systems.** `app.registerSystem(fn)` returns a `SystemId`; `commands.runSystem(idOrFn)` and `commands.runSystemWith(idOrFn, input)` (an `In<E>` param) run it when the caller's commands apply. Rust runs one with `RunTsSystem`.
- **BREAKING (semantics): a group's `runIf` is Bevy's collective `run_if`.** It is evaluated once per stage run for the whole group; `distributiveRunIf` gives today's per-member behaviour. Only stateful conditions and mid-stage changes can tell the two apart.
- **Protocol minor** (the next free one at landing): `Stage` gains the new schedules, `oneShot` and `schedule` (a custom one, named in a new `schedule` field of `SystemInfo`).
- Row "slot after transform propagation": it exists (`bevy:transform-propagate`). Only a test is added, waiting on reflect-coverage stage 3's `GlobalTransform` read.

Stage 2 (ask-first: new architectural pattern, per-frame cost):

- **TS states are real Bevy states**, through a pool of 32 slot types `TsState<N>`. Each TS machine gets `State`, `PreviousState`, `StateTransitionEvent` and Bevy's state-scoped systems. Adds `OnTransition`, `stateChanged`, `stateExists`, `PreviousState<S>`, the six state-scoped components (`DespawnOnExit`, `DespawnOnEnter`, `DisableOnExit`, `DisableOnEnter`, `EnableOnExit`, `EnableOnEnter`), `SubStates` and `ComputedStates`, and `app.ts_state::<S>()` so TS gates on a Rust state.
- Ships only after a bench measures its per-frame cost. Fallback if declined: TS-only state-scoped components handled inside `run_state_transitions`, no Rust visibility.

Stage 3 (ask-first: performance):

- **A TS predicate condition**: an exported function returning `boolean`, with system params, passed to `runIf`; also `resourceExistsAnd(R, pred)`. One JS call per evaluation, shared across a group by the collective `runIf`. Ships only after a bench. Fallback if declined: no TS predicates; a system returns early instead.

## Capabilities

### New Capabilities

- `ts-schedules`: which Bevy schedules TS systems run in, custom schedules, `runSchedule`, one-shot systems, and how they reach the protocol.
- `ts-conditions`: run conditions, combinators, the Rust condition seam and the TS predicate.
- `ts-states`: TS state machines as Bevy states, transitions, state-scoped entities, sub and computed states, Rust states in TS.

### Modified Capabilities

- `ts-systems`: "Schedules, ordering and run conditions" keeps ordering and points to `ts-schedules` and `ts-conditions`; a group's `runIf` becomes collective; "System param kinds" gains `In<E>` and `PreviousState<S>`; "States" moves to `ts-states`.
- `native-ordering`: the anchors gain one per new bridge stage and `bevy:fixed-main-loop`; slot placement accepts every stage.

## Impact

- Operator's go needed: stage 2 (new pattern: a pool of 32 monomorphised state types, about 1 to 2 µs per machine per frame, not yet measured) and stage 3 (a JS call per predicate evaluation, ending "a skipped system costs no JS call" for systems gated by one). Stage 1 needs no go.
- Code: `crates/bevy-ts/src/{manifest,systems,sets,lib,generation,messages}.rs`, a new `crates/bevy-ts/src/conditions.rs` and, in stage 2, `states.rs`; the compile step `crates/bevy-ts-cli/src/reader.rs`; both runtimes (`packages/bevy-ts/runtime/core.js`, `crates/bevy-ts-v8/js/core.js`); `packages/bevy-ts/index.d.ts`; the `native.d.ts` generator.
- Protocol: `protocol/bevy-ts.openrpc.json` `Stage` enum and `SystemInfo.schedule`, one minor per stage that changes it; regenerated Rust and TS types.
- Docs: `docs/scripts.md` (new "Schedules, conditions and states" section), `docs/native.md` (`ts_schedule`, `ts_condition`, `ts_state`, new anchors), `docs/research/bevy-coverage.md` rows.
- Neighbours: script-time and script-input-devices add conditions through the seam; ecs-queries owns the removal cursor `anyComponentRemoved` reuses; rust-events reads `StateTransitionEvent` for free after stage 2.
- No new dependency.
