// Gameplay in TS over a Rust component: `Thrust` comes from the native crate and is a column
// here because its fields are plain data (its layout is read from reflection). Editing this file
// reloads in the running engine; it never rebuilds Rust.
import { app, f32, nativeComponent, Time, Update, vec3 } from "bevy-ts";
import type { Mut, Query, Res } from "bevy-ts";

// The fields type the accessors; Rust's reflected layout places them.
export class Thrust extends nativeComponent("Thrust", { power: f32, dir: vec3 }) {}

export function throttle(time: Res<Time>, q: Query<[Mut<Thrust>]>) {
  q.each((t) => {
    t.power = 12 + Math.sin(time.elapsed * 2) * 4;
  });
}

app.addSystems(Update, throttle);
