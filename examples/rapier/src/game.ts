// Rapier with no Rust in the project: `"native": { "ports": ["rapier3d"] }` in bevy-ts.json.
// `Velocity` and `RigidBody` are Rapier's own components, columns here because their fields are
// plain data (layouts read from reflection; RigidBody's variant bytes read from this build).
import { app, enumOf, nativeComponent, vec3 } from "bevy-ts";
import { Transform } from "bevy-ts/bevy";

const Velocity = nativeComponent("Velocity", { linear: vec3, angular: vec3 });
const RigidBody = nativeComponent("RigidBody", {
  value: enumOf("Dynamic", "Fixed", "KinematicPositionBased", "KinematicVelocityBased"),
});

app.startup("spawn", ({ commands }) => {
  commands.spawn(RigidBody({ value: "Dynamic" }), Velocity, Transform);
});

app.system("launch", {
  stage: "update",
  write: [Velocity],
  run(q) {
    // Rapier's own Velocity, written in place each frame: the body climbs at 3 m/s.
    q.each((_e, v) => {
      v.linear.set(0, 3, 0);
    });
  },
});

app.system("report", {
  stage: "update",
  read: [Transform],
  with: [RigidBody],
  run(q, ctx) {
    if (ctx.frame % 60 !== 0) return;
    q.each((_e, t) => console.log(`rapier: body y=${t.translation.y.toFixed(2)}`));
  },
});
