// Adopting bevy_rapier3d end to end, from TS: TSX, a system writing velocity, contacts, a raycast.
// bevy-ts.json:  { "native": { "ports": ["rapier3d"] } }
import { app, type Entity, FixedUpdate, type MessageReader, type Mut, type Query, systems, Transform, Update, vec3, type With } from "bevy-ts";
import { CollisionEvent, PhysicsSet, type ReadRapierContext, Velocity } from "bevy-ts/rapier";
import { Enemy, Health, Player } from "./systems.ts";

const dir = vec3();
const down = vec3(0, -1, 0);

// Writes Rapier's own Velocity column in place, before the physics step.
export function steer(player: Query<[Transform], With<Player>>, enemies: Query<[Enemy, Transform, Mut<Velocity>]>) {
  const [p] = player.single();
  enemies.each((enemy, t, v) => {
    dir.subVectors(p.translation, t.translation).normalize();
    v.linear.x = dir.x * enemy.speed;
    v.linear.z = dir.z * enemy.speed;
  });
}

// Contacts: Rapier's CollisionEvent message, flattened by the port.
export function hurt(contacts: MessageReader<CollisionEvent>, players: Query<[Mut<Health>], With<Player>>) {
  contacts.each((c) => {
    if (c.kind !== "Started") return;
    const [h] = players.get(c.a) ?? players.get(c.b) ?? [];
    if (h) h.hp -= 10;
  });
}

// A raycast: one host call, read-only access declared on the Rust side.
export function grounded(rapier: ReadRapierContext, player: Query<[Entity, Transform, Mut<Velocity>], With<Player>>) {
  const [e, t, v] = player.single();
  const hit = rapier.castRay(t.translation, down, 1.1, true, { excludeRigidBody: e });
  if (!hit && v.linear.y > -20) v.linear.y -= 0.5;
}

app
  .addSystems(FixedUpdate, systems(steer, grounded).before(PhysicsSet.SyncBackend))
  .addSystems(Update, hurt);
