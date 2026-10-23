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

## Not yet built

- Writing the clocks: `ResMut<Time<Virtual>>` (`pause`, `unpause`, `toggle`, `setRelativeSpeed`,
  `setMaxDelta`) and `ResMut<Time<Fixed>>` (`setTimestep`, `setTimestepSeconds`,
  `setTimestepHz`, `discardOverstep`). Waits on writable-native-params.
- Time run conditions: `onTimer`, `onRealTimer`, `onceAfterDelay`, `repeatingAfterDelay`, their
  real forms and `paused`. Waits on ecs-schedules' `runIf`.
- `commands.delayed().secs(s)`. Waits on writable-native-params' `Commands`.

Out of scope: `TimeUpdateStrategy`, `TimeSender`/`TimeReceiver`, `Time::advance_by`.
