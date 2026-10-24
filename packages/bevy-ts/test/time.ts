// Type tests for time (docs/time.md): `tsc -p test` must pass, and every `@ts-expect-error` must
// be an error.
import {
  Component, type f32, Fixed, type Local, type Mut, type Query, Real, type Res, Resource, Stopwatch, Time, Timer,
  type TimerMode, Virtual, type Condition, onTimer, onRealTimer, onceAfterDelay, onceAfterRealDelay, repeatingAfterDelay,
  repeatingAfterRealDelay, paused, runOnce,
} from "bevy-ts";

export class Cooldown extends Component { cooldown = Timer.fromSeconds(2, "Repeating"); hp: f32 = 1 }
export class Clock extends Resource { watch = Stopwatch.new() }

// ---------- The clocks ----------

export function clocks(time: Res<Time>, real: Res<Time<Real>>, game: Res<Time<Virtual>>, fixed: Res<Time<Fixed>>) {
  const a: number = time.delta_secs + time.elapsed_secs_wrapped + time.wrap_period;
  const b: number = real.delta_secs_f64 + game.relative_speed + game.max_delta + fixed.timestep + fixed.overstep_fraction;
  const paused: boolean = game.is_paused || game.was_paused;
  void a, b, paused;
  // @ts-expect-error  `Time` alone is `Time<()>`: no virtual-clock state
  void time.is_paused;
  // @ts-expect-error  `Time<Real>` has no timestep
  void real.timestep;
  // @ts-expect-error  a clock is read-only
  time.delta_secs = 1;
}

// ---------- Timer and Stopwatch ----------

export function cool(rows: Query<[Mut<Cooldown>]>, time: Res<Time>, clock: Res<Clock>, local: Local<{ t: Timer }>) {
  for (const [c] of rows) {
    if (c.cooldown.tick(time.delta_secs).just_finished) c.hp += 1;
    c.cooldown.setMode("Once");
    const m: TimerMode = c.cooldown.mode;
    void m;
  }
  local.t.tick(time.delta_secs);
  const e: number = clock.watch.elapsed;
  void e;
  // @ts-expect-error  a read resource's stopwatch has no tick
  clock.watch.tick(1);
}

/** read_timer_has_no_tick: a read query's timer is the readonly view. */
export function readTimerHasNoTick(rows: Query<[Cooldown]>, time: Res<Time>) {
  for (const [c] of rows) {
    const left: number = c.cooldown.remaining_secs;
    void left;
    // @ts-expect-error  `tick` does not exist on the read-only view
    c.cooldown.tick(time.delta_secs);
  }
}

// @ts-expect-error  not a TimerMode
Timer.fromSeconds(1, "Forever");
export const made = Cooldown.of({ cooldown: Timer.new(0.5, "Once") });

// ---------- Time conditions ----------

export function timeConditionsTakeSeconds() {
  const all: Condition[] = [
    onTimer(2), onRealTimer(0.5), onceAfterDelay(1), onceAfterRealDelay(1), repeatingAfterDelay(3),
    repeatingAfterRealDelay(3), paused(), onTimer(1).andThen(runOnce()), paused().orElse(onRealTimer(1)),
  ];
  void all;
  // @ts-expect-error  seconds are a number
  onTimer("2");
  // @ts-expect-error  paused takes no arguments
  paused(1);
}

// `performance` is the clock only: milliseconds since the engine started.
export const sinceStart: number = performance.now();
export const origin: number = performance.timeOrigin;
