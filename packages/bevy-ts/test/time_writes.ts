// Type tests for writing the clocks (docs/time.md, Writing the clocks): the block below is what
// `bevy-ts types` writes into native.d.ts for `bevy-ts/time`'s claims (types.rs checks it holds
// `claims_ts`'s output verbatim). `tsc -p test` must pass, and every `@ts-expect-error` must be an
// error.
import type { Fixed, Res, ResMut, Time, Virtual } from "bevy-ts";

declare module "bevy-ts" {
  interface ResMutClaims {
    "ResMut<Time<Virtual>>": [Time<Virtual>, import("bevy-ts/time").VirtualTime];
    "ResMut<Time<Fixed>>": [Time<Fixed>, import("bevy-ts/time").FixedTime];
  }
}

export function menu(time: ResMut<Time<Virtual>>, fixed: ResMut<Time<Fixed>>) {
  time.pause();
  time.setRelativeSpeed(0.5);
  const paused: boolean = time.is_paused;
  const speed: number = time.relative_speed + time.delta_secs + time.max_delta;
  fixed.setTimestepHz(30);
  const step: number = fixed.timestep + fixed.overstep_fraction;
  void paused, speed, step;
  // @ts-expect-error  a property is read-only
  time.is_paused = false;
  // @ts-expect-error  the fixed clock has no pause
  fixed.pause();
}

export function reads(time: Res<Time<Virtual>>) {
  const paused: boolean = time.is_paused;
  void paused;
  // @ts-expect-error  `Res` reads; only `ResMut` writes
  time.pause();
}
