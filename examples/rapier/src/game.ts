// Rapier with no Rust in the project: `"native": { "ports": ["rapier3d"] }` in bevy-ts.json.
// `Velocity` and `RigidBody` are Rapier's own components, columns here because their fields are
// plain data (layouts read from reflection; RigidBody's variant bytes read from this build).
import { app, enumOf, nativeComponent, Time, Transform, Startup, Update, vec3 } from "bevy-ts";
import type { Commands, Mut, Query, Res, With } from "bevy-ts";

export class Velocity extends nativeComponent("Velocity", { linear: vec3, angular: vec3 }) {}
export class RigidBody extends nativeComponent("RigidBody", {
  value: enumOf("Dynamic", "Fixed", "KinematicPositionBased", "KinematicVelocityBased"),
}) {}

export function spawn(commands: Commands) {
  commands.spawn(new RigidBody({ value: "Dynamic" }), Velocity, Transform);
}

// Rapier's own Velocity, written in place each frame: the body climbs at 3 m/s.
export function launch(q: Query<[Mut<Velocity>]>) {
  q.each((v) => {
    v.linear.set(0, 3, 0);
  });
}

export function report(time: Res<Time>, q: Query<[Transform], With<RigidBody>>) {
  if (time.frame % 60 !== 0) return;
  q.each((t) => console.log(`rapier: body y=${t.translation.y.toFixed(2)}`));
}

app.addSystems(Startup, spawn).addSystems(Update, launch, report);
