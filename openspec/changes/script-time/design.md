## Context

`Res<Time>` is `RunParam::Time` (`crates/bevy-ts/src/systems.rs:449`): three `u64` words pushed
per run from `frame_info` (`runtime.rs:201`), which reads Bevy's generic `Time`. The route
catches `Time` and names ending `::Time`; `ResMut<Time>` fails the load. Nothing else from
`bevy_time` reaches TS. `advance_clock` (`runtime.rs:181`, in `First`) unpauses `Time<Virtual>`
on every running frame, so a game's own pause lasts one frame. Research:
`docs/research/script-time.md`.

Neighbours: rust-resources owns the generic `Res`/`ResMut` path and does not see `Time`.
writable-native-params owns native write calls and `Commands`. ecs-schedules owns the fixed
schedules and `runIf`. reflect-coverage owns Rust components holding Bevy's `Timer`.

## Goals / Non-Goals

**Goals:** every Bevy 0.20 time item reachable from TS by its Bevy name (table below), stage 1
built here, stages 2-4 named with what they wait on.

**Non-Goals:** `TimeUpdateStrategy`, `TimeSender`/`TimeReceiver`, `Time::advance_by` (host and
test tools). A Time-only write path. Reflecting Bevy's own `Timer` into TS columns.

## Coverage

| Bevy name | Stage | TS | Proved by |
|---|---|---|---|
| `Time` (`Time<()>`) `delta_secs`, `elapsed_secs`, `_f64` | today, 1 | `Res<Time>` | `tests/time.rs::clocks_read_bevy_values` |
| `Time::elapsed_secs_wrapped`, `_f64`, `wrap_period` | 1 | every clock | `time.rs::clocks_read_bevy_values` |
| `Time::delta`, `elapsed` (`Duration`) | 1 | same seconds as `delta_secs_f64`, `elapsed_secs_f64` | — (no second name) |
| `Time<Real>` | 1 | `Res<Time<Real>>` | `time.rs::clocks_read_bevy_values` |
| `Time<Real>` `startup`, `first_update`, `last_update` (`Instant`) | — | not visible: no seconds value | — |
| `Time<Virtual>` `is_paused`, `was_paused`, `relative_speed`, `effective_speed`, `max_delta` | 1 | `Res<Time<Virtual>>` | `time.rs::clocks_read_bevy_values` |
| `Time<Fixed>` `timestep`, `overstep`, `overstep_fraction` | 1 | `Res<Time<Fixed>>` | `time.rs::clocks_read_bevy_values` |
| `Time` swapped to `Fixed` in `FixedUpdate` | 1 (test) | `Res<Time>` in `FixedUpdate` | `time.rs::fixed_update_reads_timestep` |
| `ResMut<Time<..>>` refused | 1 | load error | `time.rs::clock_writes_fail_the_load` |
| `Time<Virtual>::pause` by the game holds | 1 | editor clock fix | `time.rs::game_pause_survives_next_frame`, `time.rs::editor_resume_keeps_game_pause` |
| `Timer` `new`, `from_seconds` | 1 | `Timer.new`, `Timer.fromSeconds` | `time.rs::timer_matches_bevy` |
| `Timer` `tick`, `is_finished`, `just_finished`, `times_finished_this_tick`, `elapsed`, `elapsed_secs`, `duration`, `mode`, `remaining`, `remaining_secs`, `fraction`, `fraction_remaining` | 1 | view properties, `tick()` | `time.rs::timer_matches_bevy`, `time.rs::once_timer_stays_finished` |
| `Timer` `pause`, `unpause`, `is_paused`, `reset`, `finish`, `almost_finish`, `set_elapsed`, `set_duration`, `set_mode` | 1 | view methods | `time.rs::timer_methods_match_bevy` |
| `TimerMode` `Once`, `Repeating` | 1 | `"Once" \| "Repeating"` | `time.rs::timer_matches_bevy` |
| `Stopwatch` `new`, `tick`, `elapsed`, `elapsed_secs`, `pause`, `unpause`, `is_paused`, `reset`, `set_elapsed` | 1 | field type | `time.rs::stopwatch_pauses` |
| Timer layout | 1 | 24 bytes, align 8 | `layout.rs::timer_and_stopwatch_layouts` |
| Timer field in protocol, inspector, `.bsn` | 1 | struct of its fields | `levels_protocol.rs::timer_field_round_trips` |
| Read-only timer has no methods | 1 | types | `typescript.rs::read_timer_has_no_tick` |
| `Time<Virtual>` `pause`, `unpause`, `toggle`, `set_relative_speed`, `set_max_delta` | 2 | `ResMut<Time<Virtual>>` methods | stage 2's tests |
| `Time<Fixed>` `set_timestep`, `set_timestep_seconds`, `set_timestep_hz`, `discard_overstep` | 2 | `ResMut<Time<Fixed>>` methods | stage 2's tests |
| `on_timer`, `on_real_timer` | 3 | `onTimer`, `onRealTimer` | stage 3's tests |
| `once_after_delay`, `once_after_real_delay`, `repeating_after_delay`, `repeating_after_real_delay` | 3 | camelCase conditions | stage 3's tests |
| `paused` | 3 | `paused` condition | stage 3's tests |
| `DelayedCommands` (`commands.delayed().secs(s)`) | 4 | `commands.delayed().secs(s)` | stage 4's tests |
| `Duration` | 1 | seconds as `number` | every row above |

Test files are under `crates/bevy-ts-cli/tests/` unless named in a crate's `src`.

## Decisions

**One param, four clocks.** `RunParam::Time` becomes `RunParam::Time(Clock)`, `Clock` one of
`Generic`, `Real`, `Virtual`, `Fixed`. Each clock has a fixed `ComponentLayout` of plain fields
(`time_layout(clock)`), filled per run into `scratch.time` as today, from the resource (`Time`,
`Time<Real>`, ...) read with Bevy's getters. No reflection, no column. The generic resource path
was rejected: `Time<Virtual>` and `Time<Fixed>` are all `Duration`s and a nested context, holes
on the layout tier, and one path for four clocks is simpler than two. `frame_info` keeps its
generic `dt`/`elapsed` for the JS frame; the clocks read the world directly.

**The route.** The reader resolves `Res<Time<Real>>` to the resource name `Time<Real>`.
`systems.rs` routes a name to `RunParam::Time` when it is `Time`, `Time<C>` or a
`bevy_time::time::Time<..>` path, `C` one of the four contexts; any other `Time<..>` fails the
load. This is the one place `Time` is special-cased, and it is ours, not rust-resources'.

**TS types.** `Time<C = void>` is a declared generic class in `index.d.ts`; `Real`, `Virtual`,
`Fixed` are declared marker classes. The type of each clock's properties is conditional on `C`, so
`time.is_paused` on `Res<Time>` is a type error. Writes are absent from the types until stage 2.

**Editor pause vs game pause.** `ScriptClock` gains `paused_virtual: bool`: true when the editor
clock (pause, a non-running frame, edit mode, `reset_clock`) paused `Time<Virtual>`. `advance_clock`
pauses only a running `Time<Virtual>` and records it, and unpauses only when `paused_virtual`.
`reset_clock`'s fresh paused `Time<Virtual>` sets it. A stepped frame while the game paused the
clock runs the stages and advances zero virtual time, which is Bevy's answer.

**Timer as a field type, not Bevy's reflected `Timer`.** `FieldType::Timer` and
`FieldType::Stopwatch` are fixed composite layouts (spec: Native layout rules), plain data, so a
Timer field is a zero-copy column with change ticks. Reflecting Bevy's `Timer` would put per-frame
reads on reflect-coverage stage 3, which waits on an operator go for its cost, and would never be
a column. The JS view is generated by `core.js`'s accessor generator like a vector view: a nested
object reading its parent's row, getters always, methods on the write class. Methods are a line-
for-line port of `bevy_time::{Timer, Stopwatch}` (0.20) in `f64` seconds; the test ticks Bevy's own
`Timer` by the same deltas and compares, so drift from the port is caught.

**Initialisers.** `reader.rs` reads `Timer.fromSeconds(2, "Repeating")`, `Timer.new(2, "Once")`
and `Stopwatch.new()` as constant defaults, as it reads `vec3(1, 2, 3)`; other expressions fail
the load with the line.

**Stage 2: writes under Bevy's names.** Scripts write `ResMut<Time<Virtual>>` and
`ResMut<Time<Fixed>>`, never a second name. Behind them are Rust params exported through
writable-native-params' mechanism into the `bevy-ts/time` module: `VirtualTime { time:
ResMut<Time<Virtual>> }` (`pause`, `unpause`, `toggle`, `set_relative_speed`, `set_max_delta`) and
`FixedTime { time: ResMut<Time<Fixed>> }` (`set_timestep`, `set_timestep_seconds`,
`set_timestep_hz`, `discard_overstep`). One synchronous host call per method, stamped with the TS
system's tick, effective in the next frame's delta as in Bevy. Each export declares the TS spelling
it stands for: `#[export(ts = "VirtualTime", module = "bevy-ts/time", stands_for =
"ResMut<Time<Virtual>>")]`, `FixedTime` claiming `ResMut<Time<Fixed>>`. On a claimed spelling the
reader emits the same port decl a `VirtualTime` param gets, so the run path is unchanged, and
`bevy-ts`'s `ResMut<T>` type resolves to the export's class (research@writable-native-params,
2026-10-23). writable-native-params owns the mechanism (`stands_for`, the registry, the reader, the
declarations); script-time owns `VirtualTime`, `FixedTime`, their claims, the `bevy-ts/time`
module's spec and the consumer requirement in stage 2's delta.

Resolution order for `Res<R>` and `ResMut<R>`, agreed with lead@rust-resources (2026-10-23):

1. `R` a `Time<..>` path (any context): script-time's route. `Res` is the stage-1 read view;
   `ResMut` is the export claiming that spelling, or a load error where none exists. Plain
   `ResMut<Time>` stays refused.
2. Otherwise a spelling an export claims with `stands_for`: that export.
3. Otherwise rust-resources' generic `Res`/`ResMut` path.

`HostParams::register` refuses two exports claiming one spelling, naming both Rust types. An
export and a native-layout resource may share a spelling: the export's claim wins, and
rust-resources' resolution skips claimed spellings (research@writable-native-params with
rust-resources, 2026-10-23). rust-resources leaves a hook point in resolution for steps 1-2; the
claims are not theirs.

Every `Res<Time<..>>` stays the stage-1 per-run value, with no host call. `VirtualTime` claims only
`ResMut<Time<Virtual>>` and `FixedTime` only `ResMut<Time<Fixed>>`; each `ResMut` class carries the
clock's read properties beside its methods (lead@script-time, 2026-10-23, both research agents
agreeing).

**Protocol and levels.** The protocol's `FieldType.kind` gains `timer` and `stopwatch`, the
next free minor at landing, additions only. A value is an object of the layout's field names
(`{ elapsed, duration, times_finished_this_tick, mode, paused, finished }`), `mode` by variant
name. Levels write it as a struct value of those fields, as they write a vector's lanes.

## Risks / Trade-offs

- `f64` seconds vs Bevy's `Duration` nanoseconds: a repeating timer's `elapsed %= duration` can
  differ in the last bits → the test compares `elapsed` within 1e-9 s, flags and counts exactly.
- A TS Timer and a Rust component's Bevy `Timer` are two types → both read alike (seconds,
  Bevy's names); docs say which is which.
- Stage 2 slipping leaves pause read-only in TS → research's fallback (queue on the TS command
  buffer) is stage 2's call, not made here.

## Migration Plan

Additions only. `Res<Time>` keeps its four fields at their offsets; the new fields follow.

## Open Questions

- Stage 2: `stands_for` is confirmed by research@writable-native-params (2026-10-23) and is a
  requirement of their design. If it does not land, stage 2 waits; it does not ship
  `vt: VirtualTime` as the TS name.
- Stage 3: ecs-schedules' condition shape for Rust-evaluated conditions with state (`on_timer` keeps
  a `Timer` per condition).
- Stage 4: whether writable-native-params' `Commands` work covers `delayed()`.
