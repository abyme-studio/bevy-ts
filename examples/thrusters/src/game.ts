// Gameplay in TS over a Rust component: `Thrust` comes from the native crate and is a column
// here because its fields are plain data (its layout is read from reflection). Editing this file
// reloads in the running engine; it never rebuilds Rust.
import { app, f32, nativeComponent, vec3 } from "bevy-ts";

// The fields type the accessors; Rust's reflected layout places them.
const Thrust = nativeComponent("Thrust", { power: f32, dir: vec3 });

app.system("throttle", {
  stage: "update",
  write: [Thrust],
  run(q, ctx) {
    q.each((_e, t) => {
      t.power = 12 + Math.sin(ctx.elapsed * 2) * 4;
    });
  },
});
