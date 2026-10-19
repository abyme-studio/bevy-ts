// Adopting bevy_rapier3d end to end, from TS: TSX, a system writing velocity, contacts, a raycast.
// bevy-ts.json:  { "native": { "ports": ["rapier3d"] } }
import { app, EntityId, FixedUpdate, Mut, query, reader, system, Transform, Update, vec3 } from "bevy-ts";
import { CollisionEvent, PhysicsSet, rapierContext, Velocity } from "bevy-ts/rapier";
import { Enemy, Health, Player } from "./systems.ts";

// A TS system writing Rapier's own Velocity column in place, before the physics step.
export const steer = system(() => {
  const player = query(Transform).with(Player);
  const enemies = query(Enemy, Transform, Mut(Velocity));
  const dir = vec3();
  return () => {
    const [p] = player.single();
    enemies.each((enemy, t, v) => {
      dir.subVectors(p.translation, t.translation).normalize();
      v.linear.x = dir.x * enemy.speed;
      v.linear.z = dir.z * enemy.speed;
    });
  };
});

// Contacts: Rapier's CollisionEvent message, flattened by the port.
export const hurt = system(() => {
  const contacts = reader(CollisionEvent);
  const players = query(Mut(Health)).with(Player);
  return () => {
    contacts.each((c) => {
      if (c.kind !== "Started") return;
      const [h] = players.get(c.a) ?? players.get(c.b) ?? [];
      if (h) h.hp -= 10;
    });
  };
});

// A raycast: one host call, read-only access declared on the Rust side.
export const grounded = system(() => {
  const rapier = rapierContext();
  const player = query(EntityId, Transform, Mut(Velocity)).with(Player);
  const down = vec3(0, -1, 0);
  return () => {
    const [e, t, v] = player.single();
    const hit = rapier.castRay(t.translation, down, 1.1, true, { exclude: e });
    if (!hit && v.linear.y > -20) v.linear.y -= 0.5;
  };
});

app
  .addSystems(FixedUpdate, steer.before(PhysicsSet.SyncBackend), grounded.before(PhysicsSet.SyncBackend))
  .addSystems(Update, hurt);
