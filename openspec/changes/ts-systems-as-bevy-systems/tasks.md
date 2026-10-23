## 1. Rebase and reconcile

- [ ] 1.1 Rebase onto main after ecs-schedules stage 1 and writable-native-params land; all cargo through `~/dev/abyme/scripts/gate-slot`
- [ ] 1.2 Rewrite the `ts-systems` delta on the archived "Schedules, ordering and run conditions" text, and add a `ts-schedules` delta replacing "one exclusive system in set `BevyTsSet::<Stage>`"; `openspec validate ts-systems-as-bevy-systems --strict`

## 2. Fetch and call

- [ ] 2.1 Split `run_one` into `fetch(cell, ticks, ...)` and `call(...)`; the exclusive paths (startup, transitions, observers, `call_entry`) build the cell from `&mut World` and keep applying output at once; existing tests pass unchanged
- [ ] 2.2 Replace each query's `QueryState<Entity>` with a `QueryState<FilteredEntityMut>` built by `QueryBuilder`; tables come from `matched_tables`

## 3. TS systems as Bevy systems

- [ ] 3.1 `crates/bevy-ts/src/bevy_system.rs`: `TsParams` (`unsafe impl SystemParam`) and `TsParamsBuilder`; `init_access` through `QueryState::init_access`, the `Res`/`ResMut` access per resource, and each exported param's kept access; an internal conflict is a load error naming both params
- [ ] 3.2 `TsOutput` buffer: `apply` runs TS output, then exported params' deferred state in param order, then observers; `set_has_deferred` only for `Commands`, `MessageWriter`, `NextState` or a deferred exported param
- [ ] 3.3 The system body: `NonSendMut<ScriptRuntime>`, Bevy's ticks, summary ticks, write marks, reader cursors, dev guard; errors and warnings to per-system stats and the log queue `pump` drains
- [ ] 3.4 Run conditions as Bevy condition systems; carry each condition's state by `(identity, tree path)` across rebuild
- [ ] 3.5 Placement: stage systems in `BevyTsSet::<Stage>`, slot systems in `BevyTsSlot` sets, chained by `TsSystemSet(identity)`; edit mode's skip as a `run_if` on those sets; remove the `stage_*` and slot exclusive systems and `run_stage`
- [ ] 3.6 Commit: diff by identity, keep unchanged systems, `remove_systems_in_set` and add for the rest, one edit per touched schedule; `set_last_run` from `SystemMemory` on every rebuilt system

## 4. Tests

- [ ] 4.1 Every scenario in the three deltas, on `FakeBackend`: the Bevy system's name and access, a conflicting Rust system reported ambiguous, a non-conflicting Rust system allowed beside, a conflicting one never overlapping, a spawn seen by the next system, a skipped system with no JS call, body-only reload with no schedule edit, a moved system editing one schedule
- [ ] 4.2 Reload keeps state: `last_run` (`Changed<T>` not refiring), `Local`s, message cursors, and ecs-schedules' `runOnce`, `resourceChanged` tick and `onTimer` time
- [ ] 4.3 writable-native-params' tick-per-run, refusal and output-before-params-order tests still pass
- [ ] 4.4 `pnpm check` and the cargo suite green through gate-slot

## 5. Docs

- [ ] 5.1 `docs/scripts.md`: TS systems are Bevy systems; ordering, sync points, ambiguity detection
- [ ] 5.2 `docs/native.md`: the "Parallel systems" row, slots as sets, ordering Rust against a TS system by `BevyTsSet`
- [ ] 5.3 README's runtime sentence; `docs/research/script-threading.md` gains a line that stage 1 landed

## 6. Measure for the operator's go

- [ ] 6.1 Bench a 20-system `Update` stage, all deferring and none deferring, against today's exclusive stage, 3 runs each, with load average noted
- [ ] 6.2 Bench a schedule rebuild after a declaration change in a 200-system `Update`
- [ ] 6.3 Report both to lead@bevy-ts with the fallback from design "Risks" if the barrier cost is too high
