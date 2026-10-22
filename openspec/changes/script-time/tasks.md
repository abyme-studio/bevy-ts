## 1. Clocks (stage 1)

- [ ] 1.1 `crates/bevy-ts/src/systems.rs`: `RunParam::Time(Clock)` with `Clock` `Generic`, `Real`, `Virtual`, `Fixed`; `time_layout(clock)` per the ts-systems delta, generic fields first at today's offsets
- [ ] 1.2 `systems.rs`: route `Time`, `Time<C>` and `bevy_time::time::Time<..>` names to `RunParam::Time`; any other context and every `ResMut` of a clock fail the load with the location
- [ ] 1.3 `systems.rs` run: fill each clock's words into `scratch.time` from the world's `Time`, `Time<Real>`, `Time<Virtual>`, `Time<Fixed>` with Bevy's getters, sized per clock
- [ ] 1.4 `crates/bevy-ts-cli/src/reader.rs`: `Res<Time<Real>>` resolves to the resource `Time<Real>` (and the other contexts)
- [ ] 1.5 `crates/bevy-ts/src/generation.rs:403`: resolve every clock's layout, not only `Time`
- [ ] 1.6 `packages/bevy-ts/index.d.ts`: generic `Time<C>`, markers `Real`, `Virtual`, `Fixed`, per-clock properties; `Res<Time>` stays valid
- [ ] 1.7 `crates/bevy-ts-cli/tests/time.rs` (new): `clocks_read_bevy_values`, `fixed_update_reads_timestep`, `clock_writes_fail_the_load`

## 2. Editor clock leaves a game pause alone (stage 1)

- [ ] 2.1 `crates/bevy-ts/src/runtime.rs`: `ScriptClock::paused_virtual`; `advance_clock` pauses only a running clock and unpauses only its own pause; `reset_clock` sets it
- [ ] 2.2 Check every other place that pauses `Time<Virtual>` (`bevy-ts-viewport/src/edit.rs`, `bevy-ts-remote/src/lib.rs`, `bevy-ts-cli/src/cli.rs`) goes through `ScriptClock`
- [ ] 2.3 `tests/time.rs`: `game_pause_survives_next_frame`, `editor_resume_keeps_game_pause`; `play_stop.rs` and `viewport/tests/edit.rs` still pass

## 3. Timer, Stopwatch, TimerMode (stage 1)

- [ ] 3.1 `crates/bevy-ts/src/layout.rs`: `FieldType::Timer` (24 bytes, align 8) and `FieldType::Stopwatch` (16, align 8) with their sub-fields; `ts_name`; unit test `timer_and_stopwatch_layouts`
- [ ] 3.2 Every exhaustive `FieldType` match compiles and does the right thing: `layout_reflect.rs`, `bevy-ts-cli/src/types.rs`, `bevy-ts-levels/src/{route,compose,writer,protocol}.rs`
- [ ] 3.3 `reader.rs`: `Timer` and `Stopwatch` as field types; `Timer.fromSeconds`, `Timer.new`, `Stopwatch.new()` as constant defaults; other initialisers fail with the line
- [ ] 3.4 `crates/bevy-ts-v8/js/core.js`: `Timer` and `Stopwatch` views in the accessor generator, getters on the read class, methods on the write class, ported line for line from `bevy_time` 0.20, one change-tick store per call, no allocation; `Timer.new`, `Timer.fromSeconds`, `Stopwatch.new` for `C.of({...})`
- [ ] 3.5 `crates/bevy-ts-v8/js/jsx-runtime.js`: a timer or stopwatch field in TSX props and the inspector as a struct of its fields
- [ ] 3.6 `index.d.ts`: `Timer`, `ReadonlyTimer`, `Stopwatch`, `ReadonlyStopwatch`, `TimerMode`; read components get the readonly forms
- [ ] 3.7 `protocol/bevy-ts.openrpc.json`: `FieldType.kind` gains `timer`, `stopwatch`; bump to the next free minor at landing; regenerate (`scripts/gen-protocol.sh`)
- [ ] 3.8 `tests/time.rs`: `timer_matches_bevy` (ticks Bevy's own `Timer` by the same deltas), `once_timer_stays_finished`, `timer_methods_match_bevy`, `stopwatch_pauses`; a timer in a resource and a `Local` too
- [ ] 3.9 `tests/typescript.rs`: `read_timer_has_no_tick`
- [ ] 3.10 `tests/levels_protocol.rs`: `timer_field_round_trips` (set through the protocol, written to `.bsn`, read back)

## 4. Docs (stage 1)

- [ ] 4.1 `docs/time.md` (new): the four clocks, what `Res<Time>` is per schedule, the editor pause vs a game pause, `Timer`/`Stopwatch` vs a Rust component's Bevy `Timer`, stages 2-4 not yet built
- [ ] 4.2 `docs/native.md:89`: `Time` is a value built per run, not a column; point at `docs/time.md`
- [ ] 4.3 The time rows (88-90, 173) of the Bevy coverage audit name stage 1 as covered and stages 2-4 as open

## 5. Gate (stage 1)

- [ ] 5.1 `openspec validate script-time --strict`
- [ ] 5.2 The repo's full check green; protocol version is the next free minor on `main` at landing

## 6. Later stages

Not tasks of this change: each is a follow-up change once its shape lands.

- Stage 2, `ResMut<Time<Virtual>>` (`pause`, `unpause`, `toggle`, `setRelativeSpeed`, `setMaxDelta`) and `ResMut<Time<Fixed>>` (`setTimestep`, `setTimestepSeconds`, `setTimestepHz`, `discardOverstep`) through exported `VirtualTime`/`FixedTime` claiming those spellings with `stands_for`, resolution order in design.md: waits on writable-native-params' export mechanism and `stands_for`
- Stage 3, `onTimer`, `onRealTimer`, `onceAfterDelay`, `onceAfterRealDelay`, `repeatingAfterDelay`, `repeatingAfterRealDelay`, `paused` wrapping Bevy's conditions: waits on ecs-schedules' `runIf` condition shape
- Stage 4, `commands.delayed().secs(s)`: waits on writable-native-params' `Commands`
