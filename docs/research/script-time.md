# Bevy's time in TS (2026-10-23, research)

Source: audit rows 88-90 and line 173 of `bevy-coverage.md`. Read from `bevy_time-0.20.0`,
`crates/bevy-ts/src/{systems,runtime,lib,layout_reflect}.rs`, `crates/bevy-ts-v8/js/core.js`,
`openspec/specs/{ts-systems,ts-components}/spec.md`, and answers from lead@writable-native-params,
lead@reflect-enums-color, lead@ecs-schedules and research@rust-resources (2026-10-23).

Today: `Res<Time>` is `RunParam::Time`, a three-word value built per run from Bevy's generic
`Time` (`systems.rs:449`, `runtime.rs:201`). Any name ending `::Time` is caught there;
`ResMut<Time>` fails the load. Nothing else from `bevy_time` reaches TS.

## 1. Bevy 0.20's time surface (the coverage table)

| Bevy name | What it is | Today in TS |
|---|---|---|
| `Time` (`Time<()>`) | generic clock; Virtual's values in `Update`, Fixed's in `FixedUpdate` | read: `delta_secs`, `elapsed_secs`, `_f64`s |
| `Time::elapsed_secs_wrapped`, `wrap_period` | wrapped elapsed for shaders | missing |
| `Time<Real>` | wall clock, never paused or scaled | missing |
| `Time<Virtual>` `is_paused`, `was_paused`, `relative_speed`, `effective_speed`, `max_delta` | game clock reads | missing |
| `Time<Virtual>` `pause`, `unpause`, `toggle`, `set_relative_speed`, `set_max_delta` | game clock writes | missing |
| `Time<Fixed>` `timestep`, `overstep`, `overstep_fraction` | fixed-step reads | missing |
| `Time<Fixed>` `set_timestep`, `_seconds`, `_hz`, `discard_overstep` | fixed-step writes | missing |
| `Time` swapped to Fixed in `FixedUpdate` | `run_fixed_main_schedule` | works by construction, no test |
| `Timer` `new`/`from_seconds`, `tick`, `is_finished`, `just_finished`, `times_finished_this_tick`, `elapsed`, `remaining`, `fraction`, `pause`, `reset`, `set_duration`, `set_mode`, `finish` | countdown | missing (hand-rolled `Local`) |
| `TimerMode` `Once`, `Repeating` | enum | missing |
| `Stopwatch` `tick`, `elapsed`, `pause`, `unpause`, `reset`, `set_elapsed` | count-up | missing |
| `on_timer`, `on_real_timer` | run conditions | missing |
| `once_after_delay`, `repeating_after_delay` and `_real_` forms | run conditions | missing |
| `paused` | run condition on `Time<Virtual>` | missing |
| `commands.delayed().secs(s)` (`DelayedCommands`) | commands applied after a delay | missing |
| `Duration` | value in all of the above | seconds as `number` (reflect-coverage) |

Out of scope: `TimeUpdateStrategy`, `TimeSender`/`TimeReceiver`, `Time::advance_by` (Rust-side
test and host tools, not gameplay).

## 2. TS names, reads and writes

Recommend: mirror Rust. `Res<Time>`, `Res<Time<Real>>`, `Res<Time<Virtual>>`,
`Res<Time<Fixed>>`, `ResMut<Time<Virtual>>`, `ResMut<Time<Fixed>>`. `bevy-ts` exports `Real`,
`Virtual`, `Fixed` marker types and a generic `Time<C>` type. Reads keep today's rule: Bevy's
getter names as snake_case properties (`delta_secs`, `is_paused`, `relative_speed`, `timestep`,
`overstep_fraction`), every `Duration` as seconds (`number`), the shape reflect-coverage uses.
Writes are Bevy's methods in TS's camelCase method style: `pause()`, `unpause()`, `toggle()`,
`setRelativeSpeed(x)`, `setMaxDelta(s)`, `setTimestepSeconds(s)`, `setTimestepHz(hz)`.

Read path: extend `RunParam::Time` to `RunParam::Time(Clock)` for the four clocks, each a value
built per run as today (a few words, no reflection). Do not fold onto the generic resource path:
`Time<()>` is layout tier (`layout_reflect.rs:536`), but `Virtual` and `Fixed` are all
`Duration`s and a nested context, which are holes there. One path for all four is simpler than
two. The route matches any `bevy_time::time::Time<..>` path, not only names ending `::Time`, so
rust-resources' generic path never sees a `Time`. research@rust-resources agreed: they own the
generic `Res`/`ResMut` path, we own everything `Time`-specific, including this route.

Write path: pause and speed are method calls, not field writes, so they are not `ResMut` column
writes. They go through writable-native-params' path. Its hypothesis (not yet decided): the
exported param's call runs inside the TS system's exclusive run, borrows the resource, writes in
place, marks changed. That fits `time.pause()` exactly and keeps Bevy's immediate semantics.
Recommend: wait for it; do not build a Time-only write path. Fallback if it slips: queue the
call on the TS command buffer (applied after the system), and say so in the docs.

Shape, from research@writable-native-params (2026-10-23): `Time<Virtual>` control is their
worked case. A Rust `#[derive(SystemParam)] struct VirtualTime { time: ResMut<Time<Virtual>> }`
with `#[bevy_ts_native::export(ts = "VirtualTime", module = "bevy-ts/time")]` methods `pause`,
`unpause`, `is_paused`, `set_relative_speed`, `relative_speed`. In TS: `vt: VirtualTime`,
`vt.pause()`. One synchronous host call per method, stamped with the TS system's tick. A negative
or NaN ratio becomes a JS throw naming `VirtualTime.setRelativeSpeed`. The change takes effect
in the next frame's delta, as in Bevy. They own the mechanism; we own `VirtualTime` (and a
`FixedTime` for `set_timestep_*`), the `bevy-ts/time` module and its spec. Spec should ask them
whether `ResMut<Time<Virtual>>` can be an alias for `VirtualTime`, so TS reads as Rust does. If
not, ship `VirtualTime`.

Reads can land before writes: yes.

A blocker found on the write side, ours to fix: `runtime::advance_clock` (in `First`) unpauses
`Time<Virtual>` on every running frame when it finds it paused. A game's `pause()`, Rust or TS,
is undone the next frame. Fix: `ScriptClock` records whether the editor's clock paused
`Time<Virtual>`, and `advance_clock` unpauses only its own pause. A stepped frame while the game
is paused then advances zero virtual time, which is right. This is Rust-only and needs no team.

## 3. FixedUpdate

A TS system in `FixedUpdate` runs today: `stage_fixed_update` is added to Bevy's `FixedUpdate`
(`lib.rs:157`), and `frame_info` reads the generic `Time`, which Bevy has swapped to
`Time<Fixed>` there. So `time.delta_secs` is the timestep. The spec says so
(`ts-systems` "Time as a resource view"); no test proves it. Stage 1 adds the test.

Schedules are ecs-schedules' (lead@ecs-schedules): they build `FixedFirst`, `FixedPreUpdate`,
`FixedPostUpdate`, `FixedLast`, and the condition mechanism (`runIf`, Rust-evaluated
conditions). They do not build any clock, the swap, Timer, Stopwatch or `on_timer`. Our
`RunParam::Time` works in their new fixed schedules unchanged, since it reads the generic `Time`.
Nothing is built twice.

## 4. Timer and Stopwatch

Bevy's `Timer` is not a component; a game wraps it (`struct Cooldown(Timer)`). TS components
take only flat fields (`ts-components`: no nested structs). So:

Recommend: `Timer` and `Stopwatch` become field types of TS components, resources and `Local`s,
like `Vec3`: a fixed plain-data layout (elapsed and duration as `f64` seconds, mode as a
`TimerMode` enum byte, `paused`, `finished`, `times_finished_this_tick: u32`) with an accessor
view whose methods port Bevy's own (`tick(time.delta_secs)`, `just_finished`, `remaining`, ...),
written in TS, allocating nothing, ticked by delta as Bevy's are. `TimerMode` is a string-literal
enum, `"Once" | "Repeating"`.

```ts
class Cooldown extends Component { timer = Timer.fromSeconds(2, "Repeating"); }
export function fire(time: Res<Time>, q: Query<Mut<Cooldown>>) {
  q.each((c) => { if (c.timer.tick(time.delta_secs).just_finished) { /* ... */ } });
}
```

Not Bevy's reflected `Timer`: lead@reflect-enums-color says a Rust struct holding `Duration`s
reads and writes through `bevy_ts::reflect_value`, `Duration` as seconds, but per-frame reads in
a TS system are reflect-coverage stage 3, waiting on an operator go for its cost, and are never
columns. A TS-layout Timer is a zero-copy column with change ticks now. A Rust component that
holds Bevy's `Timer` stays reflect-coverage's. Ours uses the same seconds-as-number shape so the
two read alike.

## 5. Where Time stops being special-cased

It stays special-cased, on purpose, in one place: the `Time<..>` route to `RunParam::Time` in
`systems.rs`. Ours. The spec line `ts-systems:134` ("a Rust resource with a native layout is
reachable as `Res<Time>` is") is rust-resources' to fix when their generic path lands; it should
stop citing `Time` as the example. `docs/native.md:89` ("`Time` is a column") is ours: it is a
value built per run.

## 6. Stages

| Stage | Contents | Needs |
|---|---|---|
| 1 | `Res<Time<Real/Virtual/Fixed>>` reads, `Time<..>` route, `elapsed_secs_wrapped`; the `FixedUpdate` test; `advance_clock` pause fix; `Timer`/`Stopwatch`/`TimerMode` field types; docs/native.md and spec wording | nothing |
| 2 | `VirtualTime` and `FixedTime` exported params (pause, speed, timestep) | writable-native-params' write path |
| 3 | `onTimer`, `onRealTimer`, `onceAfterDelay`, `repeatingAfterDelay` (and real forms), `paused` as Rust-evaluated conditions wrapping Bevy's own | ecs-schedules' condition shape (`docs/research/ecs-schedules.md`) |
| 4 | `commands.delayed().secs(s)` | ask writable-native-params, which lists `Commands` |

Recommend: spec stage 1 now as the change; stages 2-4 as follow-ups once those shapes land.

Confidence: high on what the code does today (read, not run). Medium on stages 2-4: their
shapes are other teams' and not yet written.
