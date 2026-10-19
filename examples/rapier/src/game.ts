// Rapier with no Rust in the project: `"native": { "ports": ["rapier3d"] }` in bevy-ts.json.
// `Velocity` and `RigidBody` are Rapier's own components, columns here because their fields are
// plain data (layouts read from reflection; RigidBody's variant bytes read from this build).
// `ReadRapierContext` is the port's exported param: each castRay is one call into Rust.
import { app, enumOf, nativeComponent, Startup, Transform, Update, vec3 } from "bevy-ts";
import type { Commands, Entity, Local, Mut, Query, u32, With } from "bevy-ts";
import type { ReadRapierContext } from "bevy-ts/rapier";

export class Velocity extends nativeComponent("Velocity", { linear: vec3, angular: vec3 }) {}
export class RigidBody extends nativeComponent("RigidBody", {
  value: enumOf("Dynamic", "Fixed", "KinematicPositionBased", "KinematicVelocityBased"),
}) {}

export function spawn(commands: Commands) {
  const ground = commands.spawn(RigidBody.of({ value: "Fixed" }), Transform.of({ translation: [0, -0.5, 0] }));
  commands.custom("rapier/collider", { entity: ground, cuboid: [20, 0.5, 20] });
  const body = commands.spawn(RigidBody.of({ value: "Dynamic" }), Velocity, Transform.of({ translation: [0, 2, 0] }));
  commands.custom("rapier/collider", { entity: body, ball: 0.5 });
}

// Rapier's own Velocity, written in place each frame: the body climbs at 3 m/s.
export function launch(q: Query<[Mut<Velocity>]>) {
  q.each((v) => {
    v.linear.set(0, 3, 0);
  });
}

// How high above the ground is the body? One ray down, excluding the body itself.
export function altitude(frames: Local<{ n: u32 }>, rapier: ReadRapierContext, q: Query<[Entity, Transform], With<Velocity>>) {
  if (frames.n++ % 60 !== 0) return;
  q.each((e, t) => {
    const p = t.translation;
    const hit = rapier.castRay([p.x, p.y, p.z], [0, -1, 0], 1000, true, { excludeRigidBody: e });
    console.log(`rapier: body y=${p.y.toFixed(2)} ground ${hit ? hit.toi.toFixed(2) : "none"} below`);
  });
}

app.addSystems(Startup, spawn).addSystems(Update, launch, altitude);
