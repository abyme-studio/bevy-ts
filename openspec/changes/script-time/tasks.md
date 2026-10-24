## 1. Clocks (stage 1)

- [x] 1.1 `crates/bevy-ts/src/systems.rs`: `RunParam::Time(Clock)` with `Clock` `Generic`, `Real`, `Virtual`, `Fixed`; `time_layout(clock)` per the ts-systems delta, generic fields first at today's offsets
- [x] 1.2 `systems.rs`: route `Time`, `Time<C>` and `bevy_time::time::Time<..>` names to `RunParam::Time`; any other context and every `ResMut` of a clock fail the load with the location
- [x] 1.3 `systems.rs` run: fill each clock's words into `scratch.time` from the world's `Time`, `Time<Real>`, `Time<Virtual>`, `Time<Fixed>` with Bevy's getters, sized per clock
- [x] 1.4 `crates/bevy-ts-cli/src/reader.rs`: `Res<Time<Real>>` resolves to the resource `Time<Real>` (and the other contexts)
- [x] 1.5 `crates/bevy-ts/src/generation.rs:403`: resolve every clock's layout, not only `Time`
- [x] 1.6 `packages/bevy-ts/index.d.ts`: generic `Time<C>`, markers `Real`, `Virtual`, `Fixed`, per-clock properties; `Res<Time>` stays valid
- [x] 1.7 `crates/bevy-ts-cli/tests/time.rs` (new): `clocks_read_bevy_values`, `fixed_update_reads_timestep`, `clock_writes_fail_the_load`

## 2. Editor clock leaves a game pause alone (stage 1)

- [x] 2.1 `crates/bevy-ts/src/runtime.rs`: `ScriptClock::paused_virtual`; `advance_clock` pauses only a running clock and unpauses only its own pause; `reset_clock` sets it
- [x] 2.2 Check every other place that pauses `Time<Virtual>` (`bevy-ts-viewport/src/edit.rs`, `bevy-ts-remote/src/lib.rs`, `bevy-ts-cli/src/cli.rs`) goes through `ScriptClock`
- [x] 2.3 `tests/time.rs`: `game_pause_survives_next_frame`, `editor_resume_keeps_game_pause`; `play_stop.rs` and `viewport/tests/edit.rs` still pass

## 3. Timer, Stopwatch, TimerMode (stage 1)

- [x] 3.1 `crates/bevy-ts/src/layout.rs`: `FieldType::Timer` (24 bytes, align 8) and `FieldType::Stopwatch` (16, align 8) with their sub-fields; `ts_name`; unit test `timer_and_stopwatch_layouts`
- [x] 3.2 Every exhaustive `FieldType` match compiles and does the right thing: `layout_reflect.rs`, `bevy-ts-cli/src/types.rs`, `bevy-ts-levels/src/{route,compose,writer,protocol}.rs`
- [x] 3.3 `reader.rs`: `Timer` and `Stopwatch` as field types; `Timer.fromSeconds`, `Timer.new`, `Stopwatch.new()` as constant defaults; other initialisers fail with the line
- [x] 3.4 `crates/bevy-ts-v8/js/core.js`: `Timer` and `Stopwatch` views in the accessor generator, getters on the read class, methods on the write class, ported line for line from `bevy_time` 0.20, one change-tick store per call, no allocation; `Timer.new`, `Timer.fromSeconds`, `Stopwatch.new` for `C.of({...})`
- [x] 3.5 `crates/bevy-ts-v8/js/jsx-runtime.js`: a timer or stopwatch field in TSX props and the inspector as a struct of its fields
- [x] 3.6 `index.d.ts`: `Timer`, `ReadonlyTimer`, `Stopwatch`, `ReadonlyStopwatch`, `TimerMode`; read components get the readonly forms
- [x] 3.7 `protocol/bevy-ts.openrpc.json`: `FieldType.kind` gains `timer`, `stopwatch`; bump to the next free minor at landing; regenerate (`scripts/gen-protocol.sh`)
- [x] 3.8 `tests/time.rs`: `timer_matches_bevy` (ticks Bevy's own `Timer` by the same deltas), `once_timer_stays_finished`, `timer_methods_match_bevy`, `stopwatch_pauses`; a timer in a resource and a `Local` too
- [x] 3.9 `packages/bevy-ts/test/time.ts` (the repo's type tests, under `tsc -p test`): `readTimerHasNoTick`, a `@ts-expect-error` on `tick` of a read query's timer
- [x] 3.10 `tests/levels_protocol.rs`: `timer_field_round_trips` (set through the protocol, written to `.bsn`, read back)

## 4. Docs (stage 1)

- [x] 4.1 `docs/time.md` (new): the four clocks, what `Res<Time>` is per schedule, the editor pause vs a game pause, `Timer`/`Stopwatch` vs a Rust component's Bevy `Timer`, stages 2-4 not yet built
- [x] 4.2 `docs/native.md:89`: `Time` is a value built per run, not a column; point at `docs/time.md`
- [x] 4.3 The time rows (88-90, 173) of the Bevy coverage audit name stage 1 as covered and stages 2-4 as open

## 5. Gate (stage 1)

- [x] 5.1 `openspec validate script-time --strict`
- [ ] 5.2 The repo's full check green; protocol version is the next free minor on `main` at landing

## 6. Time conditions (stage 3)

- [x] 6.1 `crates/bevy-ts/src/time_conditions.rs` (new): `expose(app)` registers the seven conditions of design.md's stage 3 table with `conditions::expose`, each factory calling Bevy's `bevy::time::common_conditions` function; `secs` checked finite and `>= 0`, else `"<tsName> takes seconds, a finite number >= 0; got <value>"`; `paused` ignores args. Called from the plugin's `build` beside `sets::expose_anchors` (`lib.rs:168`)
- [x] 6.2 `crates/bevy-ts-v8/js/core.js`: `onTimer`, `onRealTimer`, `onceAfterDelay`, `onceAfterRealDelay`, `repeatingAfterDelay`, `repeatingAfterRealDelay` (each `nativeCondition("bevy:<name>", { secs })`) and `paused()`, beside `runOnce`; `node scripts/sync-runtime.mjs` in `packages/bevy-ts` for `runtime/core.js`
- [x] 6.3 `packages/bevy-ts/index.d.ts`: the seven declarations, `(seconds: number) => Condition` and `paused(): Condition`, each with a one-line doc naming Bevy's function
- [x] 6.4 `crates/bevy-ts-cli/tests/it/time.rs`: `time_conditions_match_bevy` (each timed condition in `Update`, and `onTimer` in `FixedUpdate`, against Bevy's own gating a Rust system, `TimeUpdateStrategy::ManualDuration`, same frames), `paused_follows_the_game_clock`, `time_condition_survives_a_reload`, `a_bad_duration_fails_the_load` (file and line of the call)
- [x] 6.5 `packages/bevy-ts/test/time.ts`: `timeConditionsTakeSeconds`, a `@ts-expect-error` on `onTimer("2")` and on `paused(1)`; the conditions combine with `andThen`
- [x] 6.6 Whatever snapshot or test of `native.d.ts`'s `NativeCondition` now lists the seven `bevy:` names (`bevy-ts-cli/src/types.rs` tests, `compile.rs`)
- [x] 6.7 Docs: `docs/time.md` a "Run conditions" section (the seven, which clock each ticks, the fixed schedule, reload and Stop) and drop them from "Not yet built"; `docs/scripts.md`'s condition table gains the seven rows; `docs/native.md` says bevy-ts exposes `bevy:` time conditions as it does `bevy:` anchors; `docs/research/bevy-coverage.md` row 86 (and 214) name the conditions covered by stage 3
- [ ] 6.8 `openspec validate script-time --strict`; the repo's full check green (cargo through `~/dev/abyme/scripts/gate-slot`); no protocol change

## 7. Clock writes (stage 2)

Rebase on `main` first: writable-native-params (584b78c) and protocol 1.29.0 are there, not here.

- [ ] 7.1 `#[getter]` in `crates/bevy-ts-native-derive/src/lib.rs`: a marked `pub fn` with `&self` and no arguments becomes a property under its Rust name; arguments or `&mut self` are a compile error naming it; `crates/bevy-ts/src/host.rs` `MethodSpec` says which methods are getters
- [ ] 7.2 The port glue in `crates/bevy-ts-v8/js/core.js` defines each getter as a property (`get` over the same host call, no setter), then `node scripts/sync-runtime.mjs`; `bevy-ts-cli/src/types.rs` declares it `readonly <name>: <type>`
- [ ] 7.3 `protocol/bevy-ts.openrpc.json`: `ExportedParam.properties` (array of string, optional), the next free minor on `main` at landing (1.30.0 unless taken); regenerate (`scripts/gen-protocol.sh`); the handshake fills it
- [ ] 7.4 `crates/bevy-ts-cli/src/time.rs` (new): `ScriptTimePlugin` registers `VirtualClock` (`ts = "VirtualTime"`, `stands_for = "ResMut<Time<Virtual>>"`) and `FixedClock` (`ts = "FixedTime"`, `stands_for = "ResMut<Time<Fixed>>"`) in module `bevy-ts/time`, with design.md's method table and every getter of the clock's stage-1 list; one `secs(value)` helper over `Duration::try_from_secs_f64`; each setter returns `Err` with Bevy's message where Bevy asserts. Added beside `ScriptInputPlugin` in `cli.rs` and `types.rs`
- [ ] 7.5 `crates/bevy-ts/src/systems.rs`: the `ResMut` clock refusal names `ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>` as the clocks TS writes; it now only meets `ResMut<Time>` and `ResMut<Time<Real>>` (or a claimed clock with no plugin)
- [ ] 7.6 `crates/bevy-ts-cli/tests/it/time_writes.rs` (new, `mod time_writes;` in `main.rs`): `virtual_writes_match_bevy` (each `VirtualTime` method, the resource after the call and the next frame's `Res<Time>` delta against the same call from Rust), `fixed_writes_match_bevy` (each `FixedTime` method; `FixedUpdate` run counts against Rust), `a_write_reads_back_in_the_same_run`, `bad_clock_writes_throw` (every Bevy assert: the message, the export and method named, the clock unchanged), `ts_pause_survives_an_editor_step`, `unwritable_clocks_fail_the_load`, `clock_reads_make_no_host_call`
- [ ] 7.7 `crates/bevy-ts-cli/tests/it/exported.rs`: `a_getter_reads_as_a_property` (and its `properties` in the handshake); a `compile_fail` doc test in `crates/bevy-ts-native/src/lib.rs`, a getter that takes an argument
- [ ] 7.8 `packages/bevy-ts/test/time_writes.ts` (under `tsc -p test`): `ResMut<Time<Virtual>>` has `pause()` and `readonly is_paused`; `@ts-expect-error` on `Res<Time<Virtual>>`'s `pause`, on assigning `is_paused`, and on `ResMut<Time<Fixed>>`'s `pause`
- [ ] 7.9 Docs: `docs/time.md` a "Writing the clocks" section (both tables, when a write lands, a TS pause vs the editor's, reads as properties and `Res` beside `ResMut` for many reads) and drop stage 2 from "Not yet built"; `docs/native.md` "Calling Rust from TS" documents `#[getter]`, and "Standing for a Bevy spelling" names `VirtualTime`/`FixedTime` as shipped; `docs/research/bevy-coverage.md` row 85 (and 213) name the writes covered
- [ ] 7.10 `openspec validate script-time --strict`; the repo's full check green (cargo through `~/dev/abyme/scripts/gate-slot`); protocol version the next free minor on `main` at landing

## 8. Later stages

Not tasks of this change: a follow-up change once its shape lands.

- Stage 4, `commands.delayed().secs(s)`: waits on writable-native-params' `Commands`
