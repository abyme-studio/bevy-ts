# Time

TS reads Bevy's clocks and keeps Bevy's timers, under Bevy's names. Every `Duration` is seconds,
a `number`.

## The four clocks

```ts
import { Fixed, Real, Time, Virtual } from "bevy-ts";
import type { Res } from "bevy-ts";

export function hud(time: Res<Time>, real: Res<Time<Real>>, game: Res<Time<Virtual>>, fixed: Res<Time<Fixed>>) {
  if (game.is_paused) return;
  console.log(time.delta_secs, real.elapsed_secs, fixed.overstep_fraction);
}
```

| Param | Bevy's clock | Its own properties |
|---|---|---|
| `Res<Time>` | `Time` (`Time<()>`) | — |
| `Res<Time<Real>>` | wall clock, never paused or scaled | — |
| `Res<Time<Virtual>>` | the game clock | `is_paused`, `was_paused`, `relative_speed`, `effective_speed` (and `_f64`), `max_delta` |
| `Res<Time<Fixed>>` | the fixed-step clock | `timestep`, `overstep`, `overstep_fraction` (and `_f64`) |

Every clock has `delta_secs`, `elapsed_secs`, `elapsed_secs_wrapped`, their `_f64` forms and
`wrap_period`. `Time<Real>`'s `Instant`s (`startup`, `first_update`, `last_update`) are not
visible: they have no seconds value.

A clock is a value built for each run from the world, not a column: nothing is reflected and
nothing is written back. Writing one fails the load: `ResMut<Time<Virtual>>` says clocks are
read-only from TS.

### What `Res<Time>` is

As in Bevy, it depends on the schedule. In `Update` and the other frame schedules it holds
`Time<Virtual>`'s values. In `FixedUpdate` and every fixed schedule it holds `Time<Fixed>`'s:
`time.delta_secs` is the timestep (1/64 s by default) on every run.

## The editor's pause and a game's pause

The editor pauses with `bevy-ts/pause`, steps with `bevy-ts/step`, and pauses while in edit mode
or after a Stop. Each of these pauses `Time<Virtual>` too, and remembers that it did. It unpauses
`Time<Virtual>` only to undo its own pause.

So a game that pauses its own clock (`Time<Virtual>::pause()` from Rust) stays paused: across
frames, across an editor pause and resume, until the game unpauses it. A frame the editor steps
while the game is paused runs the bevy-ts systems and advances no game time, which is Bevy's
answer.

## Timer and Stopwatch

`Timer`, `Stopwatch` and `TimerMode` (`"Once" | "Repeating"`) are field types of components,
resources and `Local`s, ported from `bevy_time` 0.20:

```ts
import { Component, Timer, Stopwatch, Time } from "bevy-ts";
import type { Local, Mut, Query, Res } from "bevy-ts";

export class Gun extends Component { cooldown = Timer.fromSeconds(0.5, "Repeating") }

export function fire(guns: Query<[Mut<Gun>]>, time: Res<Time>, uptime: Local<{ w: Stopwatch }>) {
  uptime.w.tick(time.delta_secs);
  for (const [gun] of guns) {
    if (gun.cooldown.tick(time.delta_secs).just_finished) console.log("bang");
  }
}
```

- Make one with `Timer.fromSeconds(seconds, mode)` (an `f32`, as Bevy's), `Timer.new(seconds,
  mode)` or `Stopwatch.new()`, as a field's initialiser or in `C.of({ ... })`. An initialiser
  must be one of these with constant arguments; anything else fails the load with the line.
- A timer reads Bevy's getters: `elapsed`, `elapsed_secs`, `duration`, `mode`, `is_paused`,
  `is_finished`, `just_finished`, `times_finished_this_tick`, `remaining`, `remaining_secs`,
  `fraction`, `fraction_remaining`. A stopwatch: `elapsed`, `elapsed_secs`, `is_paused`.
- A written one (`Mut<C>`, `ResMut<R>`, a `Local`) has Bevy's methods in camelCase: a timer's
  `tick(seconds)`, `pause`, `unpause`, `reset`, `finish`, `almostFinish`, `setElapsed`,
  `setDuration`, `setMode`; a stopwatch's `tick(seconds)`, `pause`, `unpause`, `reset`,
  `setElapsed`. A read one is typed without them. Each method allocates nothing and marks the
  row changed once.
- In memory a `Timer` is 24 bytes aligned to 8: `elapsed` and `duration` (`f64` seconds),
  `times_finished_this_tick` (`u32`), `mode` (`u8`), `paused` and `finished` (`bool`). A
  `Stopwatch` is 16: `elapsed` and `paused`. The protocol, the inspector and `.bsn` files show
  either as a struct of those fields: `cooldown: Timer { duration: 2.0, mode: Repeating }`.

Seconds are `f64` where Bevy counts nanoseconds, so a repeating timer's leftover time can differ
from Bevy's in the last bits. Flags and counts match exactly.

A TS `Timer` is not Bevy's `Timer` type. A Rust component holding Bevy's own `Timer` is a Rust
component like any other (docs/native.md); its timer is not a TS timer field.

## Run conditions

Bevy's time conditions (`bevy::time::common_conditions`), under camelCase names. Each runs Bevy's
own function in Rust, through the `bevy:` conditions bevy-ts exposes (`docs/native.md`); a skipped
system costs no JS call.

| Condition | Bevy | Clock | True when |
|---|---|---|---|
| `onTimer(s)` | `on_timer` | the schedule's `Time` | each time `s` seconds pass |
| `onRealTimer(s)` | `on_real_timer` | `Time<Real>` | each time `s` seconds pass |
| `onceAfterDelay(s)` | `once_after_delay` | the schedule's `Time` | once, when `s` seconds have passed |
| `onceAfterRealDelay(s)` | `once_after_real_delay` | `Time<Real>` | once, when `s` seconds have passed |
| `repeatingAfterDelay(s)` | `repeating_after_delay` | the schedule's `Time` | every evaluation after `s` seconds |
| `repeatingAfterRealDelay(s)` | `repeating_after_real_delay` | `Time<Real>` | every evaluation after `s` seconds |
| `paused()` | `paused` | `Time<Virtual>` | the game clock is paused |

`s` is a finite number `>= 0`; anything else fails the load at the call's line. In
`FixedUpdate` the schedule's `Time` is the fixed clock: `onTimer(1)` ticks by the timestep. The
timer ticks only when the condition is evaluated, as in Bevy: while the editor pauses, the stages
do not run, so no condition ticks. Each use keeps its own timer. A reload that keeps the call and
its seconds keeps the timer; other seconds start a fresh one; a Stop clears them all.

## Delayed commands

Bevy's `commands.delayed()` (`bevy_time`'s `DelayedCommandsExt`), on every `Commands`: a system's,
an observer's, a one-shot system's, and a delayed one.

```ts
const d = commands.delayed();
const e = d.secs(1).spawn(Dummy);  // the entity now, spawned in 1 s
d.secs(2).entity(e).despawn();     // gone in 2 s
commands.delayed().duration(3).write(Wave.of({ n: 2 }));
```

- `secs(s)` rounds `s` to `f32`, as Bevy's `secs(f32)`; `duration(s)` takes seconds as every
  `Duration` does, exact to the nanosecond. Each returns a `Commands`: every method works on it,
  `delayed()` too, and nested delays add up.
- A negative, `NaN` or infinite delay throws at the call, naming the method and the value.
- Calls on one `DelayedCommands` with the same delay share one queue. When the run's commands
  apply, each queue becomes a Bevy `DelayedCommandQueue` entity with `submit_at` the `Time`
  elapsed now plus the delay. Bevy's `check_delayed_command_queues` runs it in `PreUpdate` once
  `Time` reaches `submit_at`: its commands apply then, all or nothing, warnings to the log under
  "delayed commands of <system>". Queues due on one frame have no order between them, as in Bevy.
- The clock is the game clock, `Time<Virtual>`: a game's pause and the editor's hold a delay, and
  a relative speed of 0.5 doubles it in wall time. A delay queued in `FixedUpdate` counts from
  `Time<Fixed>`'s elapsed time, as in Bevy.
- `spawn`, `spawnBatch`, `cloneAndSpawn` and `withChildren`'s `spawn` return the entity at once;
  later commands may name it. A command now on an entity a delay has not spawned yet warns that it
  does not exist.
- A run that throws drops its delayed commands with the rest. A `DelayedCommands`, or the
  `Commands` it returned, kept past its run throws on use.
- A Stop or a restart despawns every pending TS queue. A queue made before a reload applies
  nothing when it comes due, and logs one warning naming its generation and how many commands it
  held.
- Queue entities are ordinary entities: `Query<[DelayedCommandQueue]>` (`bevy-ts/bevy`) reads
  `submit_at` in seconds; the queue itself is not visible, and TS cannot insert one.

## Not yet built

- Writing the clocks: `ResMut<Time<Virtual>>` (`pause`, `unpause`, `toggle`, `setRelativeSpeed`,
  `setMaxDelta`) and `ResMut<Time<Fixed>>` (`setTimestep`, `setTimestepSeconds`,
  `setTimestepHz`, `discardOverstep`). Waits on writable-native-params.

Out of scope: `TimeUpdateStrategy`, `TimeSender`/`TimeReceiver`, `Time::advance_by`.
