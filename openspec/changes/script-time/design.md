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

**Goals:** every Bevy 0.20 time item reachable from TS by its Bevy name (table below), stages 1
and 3 built here, stages 2 and 4 named with what they wait on.

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
| Read-only timer has no methods | 1 | types | `packages/bevy-ts/test/time.ts::readTimerHasNoTick` |
| `Time<Virtual>` `pause`, `unpause`, `toggle` | 2 | `ResMut<Time<Virtual>>` `pause()`, `unpause()`, `toggle()` | `time_writes.rs::virtual_writes_match_bevy` |
| `Time<Virtual>` `set_relative_speed`, `set_relative_speed_f64`, `set_max_delta` | 2 | `setRelativeSpeed(r)`, `setRelativeSpeedF64(r)`, `setMaxDelta(s)` | `time_writes.rs::virtual_writes_match_bevy` |
| `Time<Fixed>` `set_timestep`, `set_timestep_seconds`, `set_timestep_hz`, `discard_overstep` | 2 | `ResMut<Time<Fixed>>` `setTimestep(s)`, `setTimestepSeconds(s)`, `setTimestepHz(hz)`, `discardOverstep(s)` | `time_writes.rs::fixed_writes_match_bevy` |
| `Time::set_wrap_period` on `Time<Virtual>`, `Time<Fixed>` | 2 | `setWrapPeriod(s)` on both | `time_writes.rs::virtual_writes_match_bevy`, `fixed_writes_match_bevy` |
| every clock getter on a written clock | 2 | snake_case properties of `ResMut<Time<Virtual>>`, `ResMut<Time<Fixed>>`, read live | `time_writes.rs::a_write_reads_back_in_the_same_run` |
| Bevy's setter panics (`set_max_delta(0)`, negative or non-finite speed, zero timestep or Hz, zero wrap period) | 2 | the call throws naming the export and method, the clock unchanged | `time_writes.rs::bad_clock_writes_throw` |
| a TS pause over an editor step | 2 | the game's pause holds | `time_writes.rs::ts_pause_survives_an_editor_step` |
| `ResMut<Time>`, `ResMut<Time<Real>>` | 2 | load error naming the clocks TS may write | `time_writes.rs::unwritable_clocks_fail_the_load` |
| `Res<Time<..>>` beside a written clock | 2 | stage-1 per-run view, no host call | `time_writes.rs::clock_reads_make_no_host_call` |
| clock write types | 2 | `ResMut<Time<Virtual>>` is `VirtualTime`, `Res<Time<Virtual>>` has no methods | `packages/bevy-ts/test/time_writes.ts` |
| `#[getter]` on an export | 2 | a read-only property under the Rust name | `exported.rs::a_getter_reads_as_a_property`, `bevy-ts-native` doc test `a getter takes no arguments` |
| `Time::advance_by`, `advance_to`, `Time<Fixed>::accumulate_overstep`, `Time<Real>::update*`, `context_mut`, `Time<..>::from_*`/`new_with` | — | not visible: host and test tools, or constructors of a resource Bevy owns | — |
| `on_timer`, `on_real_timer` | 3 | `onTimer(s)`, `onRealTimer(s)` | `time.rs::time_conditions_match_bevy` |
| `once_after_delay`, `once_after_real_delay`, `repeating_after_delay`, `repeating_after_real_delay` | 3 | `onceAfterDelay(s)`, `onceAfterRealDelay(s)`, `repeatingAfterDelay(s)`, `repeatingAfterRealDelay(s)` | `time.rs::time_conditions_match_bevy` |
| `on_timer` in `FixedUpdate` ticks `Time<Fixed>` | 3 | same | `time.rs::time_conditions_match_bevy` |
| `paused` | 3 | `paused()` | `time.rs::paused_follows_the_game_clock` |
| a time condition's timer across a reload | 3 | kept while the seconds stay the same | `time.rs::time_condition_survives_a_reload` |
| a bad duration | 3 | load error at the call | `time.rs::a_bad_duration_fails_the_load` |
| condition types | 3 | `(seconds: number) => Condition` | `packages/bevy-ts/test/time.ts::timeConditionsTakeSeconds` |
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
`time.is_paused` on `Res<Time>` is a type error. `Res<Time<..>>` never has writes; stage 2's
`ResMut` claims carry them.

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

**Stage 3: time conditions through the seam.** bevy-ts exposes Bevy's seven conditions from
`bevy_time::common_conditions` with `ts_condition`, under `bevy:` names, as it exposes its
`bevy:` set anchors. A new `crates/bevy-ts/src/time_conditions.rs` holds `expose_all(app)`, called
from the plugin's `build` beside `sets::expose_anchors`. Each factory calls Bevy's function and
boxes it: no port of Bevy's logic. `bevy-ts` exports a typed wrapper per condition, in camelCase
(`core.js` beside `runOnce`, typed in `index.d.ts`):

| TS | Exposed name | Bevy | Args |
|---|---|---|---|
| `onTimer(seconds)` | `bevy:on-timer` | `on_timer` | `{ secs }` |
| `onRealTimer(seconds)` | `bevy:on-real-timer` | `on_real_timer` | `{ secs }` |
| `onceAfterDelay(seconds)` | `bevy:once-after-delay` | `once_after_delay` | `{ secs }` |
| `onceAfterRealDelay(seconds)` | `bevy:once-after-real-delay` | `once_after_real_delay` | `{ secs }` |
| `repeatingAfterDelay(seconds)` | `bevy:repeating-after-delay` | `repeating_after_delay` | `{ secs }` |
| `repeatingAfterRealDelay(seconds)` | `bevy:repeating-after-real-delay` | `repeating_after_real_delay` | `{ secs }` |
| `paused()` | `bevy:paused` | `paused` | none |

A wrapper is `nativeCondition("bevy:on-timer", { secs: seconds })` and nothing else. The factory
owns the check: `secs` a finite number `>= 0`, else an error naming the TS function and the value
(`onTimer takes seconds, a finite number >= 0; got -1`), which fails the load at the call's line
(`callerLocation` skips `core.js` frames). JS's `NaN` and `Infinity` reach it as JSON `null`.
`paused()` takes no args, as `runOnce()`. A `Duration` is seconds, as in stage 1.

The seam keys an instance by its use and its JSON args, so a reload that keeps `onTimer(2)` keeps
its timer, `onTimer(3)` starts a fresh one, and a Stop clears every instance (`generation.rs`).
`Res<Time>` inside a condition is the schedule's clock, as in Bevy: `onTimer` in `FixedUpdate`
ticks by the fixed timestep. While the editor pauses, the bevy-ts stages do not run, so no
condition is evaluated; a real-time condition's next tick then adds only that frame's delta,
Bevy's answer. `NativeCondition` in `native.d.ts` lists the seven names as it lists any exposed
condition; scripts use the wrappers.

Rejected: a `Condition::Time` variant per condition in `conditions.rs`. Seven variants and a port
of each, where Bevy's own systems already run in Rust through the seam. Rejected: a
`bevy-ts/time` module for the wrappers; every other built-in condition is a `bevy-ts` export.

**Stage 2: writes under Bevy's names.** Scripts write `ResMut<Time<Virtual>>` and
`ResMut<Time<Fixed>>`, never a second name. Behind them are two params exported through
writable-native-params' mechanism (on `main`, `docs/native.md` "Standing for a Bevy spelling"):

```rust
#[derive(SystemParam)]
pub struct VirtualClock<'w> { time: ResMut<'w, Time<Virtual>> }

#[export(ts = "VirtualTime", module = "bevy-ts/time", stands_for = "ResMut<Time<Virtual>>")]
impl VirtualClock<'_> {
    pub fn pause(&mut self) { self.time.pause() }
    pub fn set_relative_speed(&mut self, ratio: f32) -> Result<(), String> { ... }
    #[getter]
    pub fn is_paused(&self) -> bool { self.time.is_paused() }
    ...
}
```

`FixedClock` (`ts = "FixedTime"`, `stands_for = "ResMut<Time<Fixed>>"`) holds
`ResMut<Time<Fixed>>`. Both live in `crates/bevy-ts-cli/src/time.rs`, whose `ScriptTimePlugin`
registers them and the `bevy-ts/time` module, added beside `ScriptInputPlugin` in `cli.rs` and
`types.rs`, as `bevy-ts/input` is. The reader already reads a claimed spelling as the export
(`reader.rs`, `a_claimed_spelling_reads_as_its_export`), and `ResMut<T>`'s type already resolves
through `ResMutClaims`; this stage adds the exports, not a route.

Methods, Bevy's names in camelCase, every `Duration` in seconds:

| Bevy | `VirtualTime` | `FixedTime` |
|---|---|---|
| `pause`, `unpause`, `toggle` | `pause()`, `unpause()`, `toggle()` | — |
| `set_relative_speed(f32)`, `set_relative_speed_f64(f64)` | `setRelativeSpeed(r)`, `setRelativeSpeedF64(r)` | — |
| `set_max_delta(Duration)` | `setMaxDelta(s)` | — |
| `set_timestep(Duration)`, `set_timestep_seconds(f64)`, `set_timestep_hz(f64)` | — | `setTimestep(s)`, `setTimestepSeconds(s)`, `setTimestepHz(hz)` |
| `discard_overstep(Duration)` | — | `discardOverstep(s)` |
| `set_wrap_period(Duration)` | `setWrapPeriod(s)` | `setWrapPeriod(s)` |

Getters, as properties under Bevy's names, read live: every clock's seven (`delta_secs`,
`elapsed_secs`, `elapsed_secs_wrapped`, their `_f64`s, `wrap_period`), then `Time<Virtual>`'s
seven or `Time<Fixed>`'s four, the stage-1 list. So `time.pause(); time.is_paused` reads `true`,
as Bevy's `is_paused()` does after `pause()`, and the two spellings read alike:
`Res<Time<Virtual>>`'s `is_paused` and `ResMut<Time<Virtual>>`'s.

Not visible, as in the Non-Goals: `advance_by`, `advance_to` and `accumulate_overstep` (Bevy: "for
use in tests"; the fixed loop owns the overstep), `Time<Real>`'s `update*` (the host's clock),
`context_mut`, and the constructors (`from_hz`, `new_with`, ...): TS writes the resource Bevy owns,
never replaces it. `ResMut<Time>` and `ResMut<Time<Real>>` fail the load, naming
`ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>` as the clocks TS writes: Bevy rebuilds `Time`
from those every frame and every fixed step, and `Time<Real>` is the wall clock.

**Bevy's panics are errors.** Each setter checks what Bevy asserts before calling it and returns
`Err` with Bevy's message (`"tried to go back in time"`, `"attempted to set fixed timestep to
zero"`, `"Hz less than or equal to zero"`, ...), so the call throws in JS naming the export and the
method and the clock is unchanged. A seconds argument goes through one helper, `secs(value) ->
Result<Duration, String>` (`Duration::try_from_secs_f64`), which refuses negative and non-finite
values. The mechanism already catches a panic, but a Bevy assert is an expected input error, not
a bug.

**When a write lands.** A method writes `Time<Virtual>` or `Time<Fixed>` in place, at the TS
system's tick. Bevy reads them in `First` (`update_virtual_time`) and in
`run_fixed_main_schedule`, so a pause or speed takes effect in the next frame's delta, and a
timestep set in `FixedUpdate` takes effect on the next fixed step, as from a Rust system.

**The editor clock.** A TS `pause()` is a game pause: it never touches
`ScriptClock::paused_virtual`, so stage 1's rule holds unchanged. Over an editor step
(`advance_clock` cleared `paused_virtual` and unpaused for the stepped frame), a TS `pause()` leaves
the clock paused, the next held frame finds it paused and records nothing, and resume leaves it
paused. A TS `unpause()` runs only in a running or stepped frame, so the editor never has its own
pause undone. No change to `runtime.rs`.

**Getters on exports: `#[getter]`.** `#[export]` makes every `pub fn` a camelCase method today; it
has no property. A `pub fn` marked `#[getter]`, `&self` and no arguments, becomes a read-only
property under its Rust name (snake_case, as the stage-1 views and Bevy's getters), one host call
per read. The macro refuses a getter that takes arguments or `&mut self`, naming it. Declarations
say `readonly is_paused: boolean`; the handshake's `ExportedParam` gains `properties: string[]`
beside `methods`, an addition in the next free protocol minor (1.30.0 if nothing takes it first).
This extends writable-native-params' mechanism, now on `main` with no owner team; script-time
owns the extension. Rejected: getter methods (`time.isPaused()`), which would make
`Res<Time<Virtual>>` and `ResMut<Time<Virtual>>` read differently; a per-run snapshot on the
export, which reads stale after `pause()`; merging the stage-1 view into the port value, which
changes `run_one`'s glue writable-native-params kept fixed.

Resolution order for `Res<R>` and `ResMut<R>`, agreed with lead@rust-resources (2026-10-23):

1. `R` a `Time<..>` path (any context): script-time's route. `Res` is the stage-1 read view;
   `ResMut` is the export claiming that spelling, or a load error where none exists. Plain
   `ResMut<Time>` stays refused.
2. Otherwise a spelling an export claims with `stands_for`: that export.
3. Otherwise rust-resources' generic `Res`/`ResMut` path.

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
- A getter is a host call per read, where `Res<Time<..>>` is a per-run value → a system reading
  many clock values each frame takes `Res<Time<Virtual>>` beside the `ResMut`; docs say so. A host
  call is a V8 fast op; seven reads a frame are noise.
- A Rust system pausing `Time<Virtual>` while the editor holds it paused is a no-op Bevy pause,
  and the editor's resume undoes it. TS cannot hit this (its stages are stopped); stage 1's rule,
  unchanged here.

## Migration Plan

Additions only. `Res<Time>` keeps its four fields at their offsets; the new fields follow.

## Open Questions

- Stage 4: whether writable-native-params' `Commands` work covers `delayed()`.
