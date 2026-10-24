## 1. Stage 1: schedules, conditions, one-shot systems

- [x] 1.1 `manifest.rs`: `Stage` gains `First`, `RunFixedMainLoop`, `SpawnScene`, `Last`, `FixedFirst`, `FixedPreUpdate`, `FixedPostUpdate`, `FixedLast`, `Custom(label)`; `ScheduleDecl` gains `PreStartup`, `PostStartup`, `OneShot`; a `ScheduleOrderDecl` list; `ConditionDecl` gains every condition and combinator of ts-conditions plus `Native { name, args }`; system groups with a collective condition
- [x] 1.2 `index.d.ts`: the `Schedule` consts, `ScheduleLabel`, `schedule(name)`, `initSchedule`, the six `insert*Schedule*` methods, `registerSystem`/`SystemId`, `In<E>`, `commands.runSchedule`/`runSystem`/`runSystemWith`, every condition, the combinator methods on `Condition`, `nativeCondition`, `distributiveRunIf`, the `With`/`Without`/`Added`/`Changed` filter values
- [x] 1.3 Both runtimes (`packages/bevy-ts/runtime/core.js`, `crates/bevy-ts-v8/js/core.js`): manifest output for the above, the new command opcodes, group conditions instead of copying them onto members
- [x] 1.4 Compile step (`crates/bevy-ts-cli/src/reader.rs`): read `registerSystem` arguments as systems; `In<E>` first-param rule; `ScheduleLabel` classes named like components
- [x] 1.5 `sets.rs`/`systems.rs`: lazily made stages (`MadeStages`, one schedule edit at commit), the `First` stage after `runtime::pump`, `RunFixedMainLoop` in `BeforeFixedMainLoop`, `Time<Fixed>` for fixed stages; anchors `bevy-ts:<stage>` and `bevy:fixed-main-loop`; slot placement for every stage
- [x] 1.6 `generation.rs`: `PreStartup`, `Startup`, `PostStartup` lists in order, commands applied between
- [x] 1.7 `TsSchedule` label, `app.ts_schedule`, `MainScheduleOrder`/`FixedMainScheduleOrder`/startup order reconciled at commit, `NativeSchedule` in `native.d.ts`
- [x] 1.8 Stage runner holds only its own list out of `TsSystems`; `runSchedule` and `runSystem`/`runSystemWith` applied with the caller's commands; re-entry logged and skipped; `RunTsSystem` command for Rust
- [x] 1.9 New `crates/bevy-ts/src/conditions.rs`: compile and evaluate every condition, `ConditionState` per config and tree path in `SystemMemory`, kept across reload, reset on Stop and `restart`; `anyComponentRemoved` on the removal cursor (build it here if ecs-queries has not landed it, then hand it over)
- [x] 1.10 `app.ts_condition` registry, factory run at commit, load errors naming the call, `NativeCondition` in `native.d.ts`
- [x] 1.11 Collective `runIf` evaluated once per stage run per group; `distributiveRunIf` per member
- [x] 1.12 Protocol: `Stage` values and `SystemInfo.schedule` in `protocol/bevy-ts.openrpc.json`, next free minor at landing, regenerate Rust and TS types, `bevy-ts/systems` fills them
- [x] 1.13 Tests (`crates/bevy-ts-cli/tests/schedules.rs`, `conditions.rs`): one per ts-schedules and ts-conditions scenario, the collective/distributive scenarios, `a_skipped_system_costs_no_js_call` for every Rust condition, a test that an unused schedule adds no bevy-ts system, a protocol test for `stage: "schedule"`
- [x] 1.14 Test for the after-propagate slot (native-ordering "After propagation"): written and `#[ignore]`d, waiting on reflect-coverage stage 3's `GlobalTransform` read; un-ignore when that lands
- [x] 1.15 Docs: `docs/scripts.md` "Schedules, conditions and states" (schedules, custom schedules, conditions, combinators, one-shot systems, collective `runIf`); `docs/native.md` (`ts_schedule`, `ts_condition`, new anchors, `RunTsSystem`)
- [x] 1.16 Coverage matrix `docs/research/bevy-coverage.md` (rebase onto main first; it is not on this branch): schedules row, conditions row (all but the TS predicate idiomatic), one-shot row, `GlobalTransform` row's slot part marked existing with the ignored test; new rows for `RunFixedMainLoop`/`SpawnScene`, combinators and the condition seam
- [x] 1.17 Tell script-time and script-input-devices the seam is in (`ts_condition`, `nativeCondition`)
- [x] 1.18 Sync stage 1 specs into `openspec/specs` (ts-schedules, ts-conditions but the predicate, native-ordering, ts-systems ordering and `In<E>`); the deltas keep only stages 2 and 3

## 2. Stage 2: TS states as Bevy states (operator's go needed)

- [x] 2.1 Bench first (`bench/states`, outside the workspace per `docs/testing.md`; results in `bench/results/states-bench.txt`): a prototype pool with 1, 8 and 32 machines, quiet frames and transition frames, µs per frame; build time and binary size of 16 vs 32 slots. Reported; go given for the 32-slot pool with `StateTransition` on the single-threaded executor
- [ ] 2.1a `StateTransition` on `SingleThreadedExecutor` in `BevyTsPlugin`; docs say transition systems run on one thread and show the opt-out; a test of the opt-out
- [ ] 2.2 `crates/bevy-ts/src/states.rs`: `TsState<N>(Sym)`, value interning, slot assignment by path, per-slot registration at run time (resources, message, `register_state`, state-scoped systems, component reflect), `TsStates::slot`, the 32 limit error and its test
- [ ] 2.3 Bevy drives transitions: TS `OnEnter`/`OnExit` systems run from Bevy's `OnEnter`/`OnExit(TsState::<N>(v))` schedules; TS `next.set` and Rust `NextState<TsState<N>>` alike; pending values held while paused; a removed value transitions to `initState`
- [ ] 2.4 `OnTransition`, `PreviousState<S>`, `stateChanged`, `stateExists` (manifest, runtimes, `index.d.ts`, compile step)
- [ ] 2.5 The six state-scoped components in TSX, commands and levels, mapped to the slot on insert
- [ ] 2.6 `SubStates` and `ComputedStates`: declaration, source order, compute called only on a source transition
- [ ] 2.7 `app.ts_state::<S>()`, `EngineMode` registered, Rust states in `native.d.ts`
- [ ] 2.8 Protocol: `onTransition` in `Stage`, next free minor; regenerate
- [ ] 2.9 Tests (`crates/bevy-ts-cli/tests/it/states.rs`): one per ts-states scenario; the existing `entering_playing_runs_exit_then_enter_and_unlocks_systems` still passes
- [ ] 2.10 Docs (`docs/scripts.md` states part with the 32-machine limit, the single-threaded `StateTransition` and its opt-out, `docs/native.md` `ts_state`) and coverage rows (sub/computed/previous/transition, state-scoped entities)

## 3. Stage 3: TS predicate conditions (operator's go needed)

- [ ] 3.1 Bench first (`crates/bevy-ts-cli/tests/predicate_bench.rs`): one predicate gating 1 and 8 systems, collective and distributive, µs per frame against a Rust condition. Report to the operator with the fallback; stop here until they say go
- [ ] 3.2 Compile step: a predicate is an exported function returning `boolean` with read-only params; writing params fail the load
- [ ] 3.3 Evaluate through `run_one` with the return read back; Rust nodes first where the combinator allows; `resourceExistsAnd(R, pred)`
- [ ] 3.4 Tests: ts-conditions predicate scenarios; a group shares one call; a system gated only by Rust conditions still costs no JS call
- [ ] 3.5 Docs and the conditions coverage row's TS predicate part
