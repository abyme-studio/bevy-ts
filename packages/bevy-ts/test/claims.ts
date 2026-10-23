// Type tests for `stands_for`: the block below is what `bevy-ts types` writes into native.d.ts for
// an export claiming `ResMut<Wind>` (types.rs checks it holds `claims_ts`'s output verbatim).
import { type f32, Resource, type Res, type ResMut, type Time, type Virtual, type Fixed } from "bevy-ts";

export class Wind extends Resource { speed: f32 = 0 }
export class Calm extends Resource { still: f32 = 0 }

declare module "bevy-ts" {
  interface ResMutClaims {
    "ResMut<Wind>": [Wind, import("bevy-ts/claims-test").WindEdit];
    "ResMut<Time<Virtual>>": [Time<Virtual>, import("bevy-ts/claims-test").VirtualTime];
  }
}

// The claimed spelling is the export.
export function blow(w: ResMut<Wind>) { w.gust(2); }
// @ts-expect-error: the export has no field `speed`
export function wrong(w: ResMut<Wind>) { w.speed = 1; }
// An unclaimed spelling stays the resource.
export function calm(c: ResMut<Calm>) { c.still = 1; }
// A clock claim: `ResMut<Time<Virtual>>` is the export; other clocks and reads stay clocks.
export function menu(time: ResMut<Time<Virtual>>) { time.pause(); }
// @ts-expect-error: Time<Fixed> is not claimed, so it has no `pause`
export function fixed(time: ResMut<Time<Fixed>>) { time.pause(); }
export function read(time: Res<Time<Virtual>>) { return time.delta_secs; }
