// Type tests for `stands_for`: the block below is what `bevy-ts types` writes into native.d.ts for
// an export claiming `ResMut<Wind>` (types.rs checks it holds `claims_ts`'s output verbatim).
import { type f32, Resource, type Res, type ResMut, type Time, type Virtual } from "bevy-ts";

export class Wind extends Resource { speed: f32 = 0 }
export class Calm extends Resource { still: f32 = 0 }

declare module "bevy-ts" {
  interface ResMutClaims {
    "ResMut<Wind>": [Wind, import("bevy-ts/claims-test").WindEdit];
  }
}

// The claimed spelling is the export.
export function blow(w: ResMut<Wind>) { w.gust(2); }
// @ts-expect-error: the export has no field `speed`
export function wrong(w: ResMut<Wind>) { w.speed = 1; }
// An unclaimed spelling stays the resource.
export function calm(c: ResMut<Calm>) { c.still = 1; }
// The clocks' claims are the engine's own: time_writes.ts.
export function read(time: Res<Time<Virtual>>) { return time.delta_secs; }
