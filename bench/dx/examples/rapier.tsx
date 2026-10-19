// Adopting bevy_rapier3d end to end, from TS: TSX, a system writing velocity, contacts, a raycast.
// bevy-ts.json:  { "native": { "ports": ["bevy-ts-rapier"] } }
import {
  app, Commands, EntityId, FixedUpdate, Mut, MessageReader, Query, Res, system, Time, Transform, Update, vec3,
} from "bevy-ts";
import { CollisionEvent, PhysicsSet, RapierContext, Velocity } from "bevy-ts/rapier";
import { Enemy, Health, Player } from "./systems.ts";

const dir = vec3();
const down = vec3(0, -1, 0);

// A TS system writing Rapier's Velocity in place, ordered before the physics step.
export const steer = system(
  {
    time: Res(Time),
    player: Query([Transform], { with: [Player] }),
    enemies: Query([Enemy, Transform, Mut(Velocity)]),
  },
  ({ player, enemies }) => {
    const [p] = player.single();
    enemies.each((enemy, t, v) => {
      dir.subVectors(p.translation, t.translation).normalize();
      v.linear.x = dir.x * enemy.speed;
      v.linear.z = dir.z * enemy.speed;
    });
  },
);

// Contacts: Rapier's CollisionEvent message, flattened by the port.
export const hurt = system(
  { contacts: MessageReader(CollisionEvent), q: Query([Mut(Health)], { with: [Player] }), commands: Commands },
  ({ contacts, q }) => {
    contacts.each((c) => {
      if (c.kind !== "Started") return;
      const [h] = q.get(c.a) ?? q.get(c.b) ?? [];
      if (h) h.hp -= 10;
    });
  },
);

// A raycast: one host call, read-only access declared on the Rust side.
export const grounded = system(
  { rapier: RapierContext, q: Query([EntityId, Transform, Mut(Velocity)], { with: [Player] }) },
  ({ rapier, q }) => {
    const [e, t, v] = q.single();
    const hit = rapier.castRay(t.translation, down, 1.1, true, { exclude: e });
    if (!hit && v.linear.y > -20) v.linear.y -= 0.5;
  },
);

app
  .addSystems(FixedUpdate, steer.before(PhysicsSet.SyncBackend), grounded.before(PhysicsSet.SyncBackend))
  .addSystems(Update, hurt);
