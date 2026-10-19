// Rapier with no Rust in the project: `"native": { "ports": ["rapier3d"] }` in bevy-ts.json.
// `Velocity` and `RigidBody` are Rapier's own components, columns here because their fields are
// plain data (layouts read from reflection; RigidBody's variant bytes read from this build).
// `bevy-ts/rapier` is the port's module: `Collider` constructors, `CollisionEvent` flattened,
// and `ReadRapierContext`, an exported param whose every castRay is one call into Rust.
import { app, enumOf, nativeComponent, Startup, Transform, Update, vec3 } from "bevy-ts";
import type { Commands, Entity, Local, MessageReader, Mut, Query, u32, With } from "bevy-ts";
import { Collider, CollisionEvent } from "bevy-ts/rapier";
import type { ReadRapierContext } from "bevy-ts/rapier";

export class Velocity extends nativeComponent("Velocity", { linear: vec3, angular: vec3 }) {}
export class RigidBody extends nativeComponent("RigidBody", {
  value: enumOf("Dynamic", "Fixed", "KinematicPositionBased", "KinematicVelocityBased"),
}) {}

// The ground as a scene: `<Collider>` is the port's element for Rapier's unreflected Collider.
app.scene("ground", () => (
  <entity name="ground">
    <RigidBody value="Fixed" />
    <Transform translation={[0, -0.5, 0]} />
    <Collider cuboid={[20, 0.5, 20]} />
  </entity>
));

export function spawn(commands: Commands) {
  const rocket = commands.spawn(RigidBody.of({ value: "Dynamic" }), Velocity, Transform.of({ translation: [0, 2, 0] }));
  Collider.ball(commands, rocket, 0.5);
  // A rock that falls onto the ground: one collision.
  const rock = commands.spawn(RigidBody.of({ value: "Dynamic" }), Transform.of({ translation: [4, 3, 0] }));
  Collider.ball(commands, rock, 0.5);
}

// Rapier's own Velocity, written in place each frame: the rocket climbs at 3 m/s.
export function launch(q: Query<[Mut<Velocity>]>) {
  q.each((v) => {
    v.linear.set(0, 3, 0);
  });
}

// How high above the ground is the rocket? One ray down, excluding the rocket itself.
export function altitude(frames: Local<{ n: u32 }>, rapier: ReadRapierContext, q: Query<[Entity, Transform], With<Velocity>>) {
  if (frames.n++ % 60 !== 0) return;
  q.each((e, t) => {
    const p = t.translation;
    const hit = rapier.castRay([p.x, p.y, p.z], [0, -1, 0], 1000, true, { excludeRigidBody: e });
    console.log(`rapier: rocket y=${p.y.toFixed(2)} ground ${hit ? hit.toi.toFixed(2) : "none"} below`);
  });
}

export function collisions(events: MessageReader<CollisionEvent>) {
  events.each((c) => console.log(`rapier: collision ${c.kind} ${c.a} ${c.b} sensor=${c.sensor}`));
}

app.addMessage(CollisionEvent).addSystems(Startup, spawn).addSystems(Update, launch, altitude, collisions);
