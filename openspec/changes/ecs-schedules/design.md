## Context

Research: `docs/research/ecs-schedules.md` (today's code, Bevy 0.20's names, costs, neighbours). In short: TS reaches four stages plus `Startup`, `OnEnter`, `OnExit`; conditions are `InState`, `ResourceExists`, `OnMessage`, `Not`, `All`, evaluated in Rust in `systems.rs` `evaluate`; TS states live in `TsPayloads` and one exclusive system, `run_state_transitions`, drives them; slots around exposed sets are made lazily (`sets.rs` `ensure_slots`).

Each stage lands as its own commit set on this branch; stage 1 lands first and alone if the operator holds 2 or 3.

## Goals / Non-Goals

Goals: every Bevy schedule a game adds systems to, every Bevy run condition, one-shot systems, and TS states that are Bevy states, each under Bevy's name. An unused feature costs nothing per frame.

Non-goals: exclusive world access from TS (declined, JS stays batched); `Main`, `FixedMain` and `StateTransition` as TS schedules; reflect-tier resources in `resourceEquals` (rust-resources); `Or` filters (ecs-queries).

## Decisions

### Stage 1

1. Stages per schedule, made lazily. `Stage` gains `First`, `RunFixedMainLoop`, `SpawnScene`, `Last`, `FixedFirst`, `FixedPreUpdate`, `FixedPostUpdate`, `FixedLast` and `Custom(label)`. The four existing stages stay eager, because Rust plugins already order against their `BevyTsSet`. The others are added to `Schedules` the first time a committed generation has a system in them, by the same code path as `ensure_slots` (one edit at the frame boundary, recorded in a `MadeStages` set). Rejected: all stages eager, which puts eight more exclusive barriers in every frame (`docs/research/slot-barrier.md`).

2. `PreStartup`/`PostStartup` are lists in `TsSystems` beside `startup`, run by `run_startups` in that order. Bevy's own `PreStartup`/`PostStartup` schedules are not used: TS startup runs at the generation apply so it survives reload, and must keep doing so.

3. Custom schedules. `bevy_ts::TsSchedule(&'static str)` derives `ScheduleLabel`; the path is leaked once per label for the app's life (labels are few). `MainScheduleOrder`/`FixedMainScheduleOrder` positions come from the manifest (`ScheduleOrderDecl { label, after|before, order: main|fixed|startup }`) and are reconciled at commit: remove the label, then insert at the declared place. `app.ts_schedule(name, label)` stores an `InternedScheduleLabel` in a `TsSchedules` registry beside `TsSets`.

4. Nested runs. `runSchedule` and `runSystem` are new command opcodes. Today `with_systems` takes the whole `TsSystems` out of the world, so a nested stage would find nothing. The stage runner changes to hold only its own list out (`stages.remove(key)`, as it already does inside) and put `TsSystems` back before applying a system's commands. A nested run of the schedule being run fails Bevy's `try_run_schedule` (the schedule is out of `Schedules`); bevy-ts logs it with the system's name. One-shot systems live in `TsSystems.one_shot`, keyed by identity, taken out while running and put back before the runs their commands queue, so a one-shot that runs itself recurses, as Bevy's `run_system` does; nesting past 64 runs logs an error and stops.

5. `SystemId` in TS is a branded identity string; `runSystem(fn)` reads the function's identity. No `run_system_cached`: every one-shot is registered so the compile step reads its params from the `registerSystem` call, as it reads `addSystems`. `In<E>` reuses event encoding (`commands.trigger`), so no new layout path. Rust's `RunTsSystem` is a `Command` that queues the identity and bytes onto `TsPayloads`; the next bevy-ts stage (or the current one, if applying TS commands) runs it.

6. Conditions. `ConditionDecl` gains one variant per condition and combinator; `Condition` gains matching compiled forms. State lives in a `ConditionState` table in `SystemMemory`, keyed by `(config identity, path in the condition tree)`, holding a `Tick`, a removal cursor or a `bool`. The config identity is the system's identity for a single system and the group's first member plus a group index for a collective `runIf`. `Changed`/`Added` compare the resource entity's component ticks with the condition's own last tick, then set it to the current change tick, as Bevy's condition system's `last_run`. `resourceEquals` compares the resource's column bytes with the encoded value; padding is zeroed on both sides at encode. `anyWithComponent` checks for a non-empty matching archetype; `anyMatchFilter` builds a `QueryState` once per generation.

7. The seam. `app.ts_condition(name, factory)` with `factory: fn(&serde_json::Value) -> Result<BoxedCondition, String>`. Compiled to `Condition::Native(Box<dyn ReadOnlySystem<In = (), Out = bool>>)`, initialized at commit, run with `run_readonly` on `&mut World` (we are always exclusive). Its own `Local`s and `last_run` are its state; reload makes a fresh instance only when the call's args change.

8. Collective `runIf`. Today `core.js:1105` copies a group's conditions onto each member. The manifest gains `groups: [{ id, members, condition }]`; the stage runner evaluates a group condition before its first member runs in that stage run and caches the result for the rest. `distributiveRunIf` keeps the current path. This is the shape stage 3 needs to share one predicate call across a group.

9. Protocol: `Stage` enum values and `SystemInfo.schedule`, one minor, regenerated. Today's file is 1.24.0; the landing takes whatever minor is next on main then.

### Stage 2: TS states as Bevy states

10. `TsState<const N: u8>(Sym)`, 32 slots, `Sym` an interned value name. A slot is registered with Bevy's own machinery at run time, the first time a machine takes it, so an unused slot costs nothing: `State`, `NextState`, the `StateTransitionEvent` message, `register_state` (Bevy then keeps `PreviousState`) and the nine state-scoped systems (`bevy_state/src/app.rs:244-275`). Bevy drives transitions. TS `OnEnter`/`OnExit`/`OnTransition` systems run from one exclusive system bevy-ts adds to Bevy's `OnEnter(TsState::<N>(v))` (and `OnExit`, `OnTransition`) schedule the first time a generation has systems there, so they are ordered with Rust's systems in the same schedules, deepest machine first on exit as Bevy orders them. TS writes go to `NextState<TsState<N>>`; while the bevy-ts stages are paused, a system before `DependentTransitions` holds pending values back so no transition runs unseen. No Bevy `SubStates`/`ComputedStates` impls: sources are types, so pairs would be combinatorial; bevy-ts computes them in `DependentTransitions`, in source order.

    `StateTransition` runs on Bevy's `SingleThreadedExecutor`, set by `BevyTsPlugin` (bench 2.1, `bench/states`, `bench/results/states-bench.txt`: on the multi-threaded executor each machine's dozen tiny systems cost 8 to 19 µs per machine per frame at 32 machines, the first machine alone 48 to 79 µs; single-threaded 0.3 to 0.5 µs quiet and 2.5 to 3 µs on transition frames; load 15 to 33). It applies to every app's `StateTransition`, Rust states included: `OnEnter`/`OnExit` systems run on one thread. A game with heavy transition work opts back in with Bevy's own call after adding the plugin: `app.edit_schedule(StateTransition, |s| { s.set_executor(MultiThreadedExecutor::new()); })`.

11. Pool size 32: one machine per slot, sub and computed included. Past it the load fails with "at most 32 state machines", naming the machine. The bench measured the pool's build at 25 to 35 s for both 16 and 32 slots (noise) and its binary at 7.5 MB (16) and 12.2 MB (32); 32 stays.

12. TSX and levels name state-scoped components by their reflect path with the state value as a string; bevy-ts maps `game::Game` to its slot on insert, so the component on the entity is Bevy's own.

13. `app.ts_state::<S>()` registers a Rust state with an adapter that maps its unit variants to names (reflect-enums-color's shape); it costs nothing per frame beyond what Bevy already runs for `S`.

### Stage 3: TS predicate

14. A predicate is a system declaration with `returns: "bool"` and read-only params, run through `run_one` with its result read from the call's return. One call per evaluation; a collective `runIf` makes it one per group per stage run. Rust-evaluated conditions in the same tree run first where the combinator allows, so `inState(X).andThen(pred)` costs no call outside `X`.

## Risks / Trade-offs

- Per-frame cost, stage 1: one exclusive system per new schedule a game uses, nothing for unused ones. Rust conditions are nanoseconds. Measured by the existing `a_skipped_system_costs_no_js_call` and a new barrier count test.
- Per-frame cost, stage 2: about 1 to 2 µs per machine per frame for six early-return systems and one message type, estimated. Bench before it ships (task 2.1): 1, 8 and 32 machines, quiet frames and transition frames, plus build time and binary size of the pool. Fallback if declined: state-scoped components as TS-only components applied inside `run_state_transitions` (no per-frame cost, no Rust visibility, no `PreviousState` for Rust); `OnTransition`, `stateChanged`, `PreviousState<S>` in TS and sub/computed states can still be built on `TsPayloads`.
- Per-frame cost, stage 3: about 3 µs per predicate evaluation (`docs/research/change-detection.md:130`), every frame its stage runs. Bench before it ships (task 3.1): one predicate on 1 and 8 systems, collective and distributive. Fallback if declined: no TS predicates; a system tests its own params and returns early, which costs the same call but is explicit.
- Collective `runIf` changes behaviour for a group gated by a stateful condition or by one a member's commands flip. Every gated group in the repo today (`packages/bevy-ts/test/systems.ts`, `bench/dx/examples`) uses `inState`, which cannot change inside a stage, so none changes behaviour.
- Custom schedule labels leak one string each; bounded by the labels a project declares.
- Spec overlap: this change and ts-systems-as-bevy-systems both MODIFY ts-systems "Schedules, ordering and run conditions". Whichever archives second takes the other's text. The specs name only the set `BevyTsSet::<Stage>`, not one exclusive system, so both changes hold.

## Answers to the research's open questions

1. Pool size: 32, load error naming the limit past it; the bench may lower it to 16.
2. A removed value on reload: a transition to the `initState` value, exits applying (ts-states).
3. Rust states in TS: yes, `app.ts_state::<S>()`, stage 2; `EngineMode` registered.
4. Rust runs a TS one-shot: yes, `RunTsSystem`; input: yes, `In<E>` with an `Event` class.
5. Eager combinators: yes, all four; they cost nothing extra.
6. The predicate ships only behind its bench and the operator's go: theirs to decide.
7. `resourceEquals` on reflect-tier resources: waits for rust-resources.

## Migration Plan

No migration for games except the collective `runIf`. Protocol additions only. Rollback per stage is reverting its commits; no stored data changes shape (stage 2's interned values replace indices in memory only).

## Open Questions

For the operator: the go for stage 2 and for stage 3, each after its bench.
