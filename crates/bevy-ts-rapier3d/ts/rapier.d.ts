import type { Commands, Entity, Message } from "bevy-ts";
/** Rapier's `CollisionEvent`, flattened: read it with `MessageReader<CollisionEvent>` after `app.addMessage(CollisionEvent)`. */
export declare class CollisionEvent extends Message {
  static readonly path: "bevy_rapier3d::pipeline::events::CollisionEvent";
  kind: "Started" | "Stopped";
  a: Entity;
  b: Entity;
  sensor: boolean;
}
export interface ColliderOptions { sensor?: boolean }
/** `Collider` constructors: Rapier's `Collider` has no `Reflect`, so the port inserts it. */
export declare const Collider: {
  ball(commands: Commands, entity: Entity, radius: number, opts?: ColliderOptions): void;
  cuboid(commands: Commands, entity: Entity, hx: number, hy: number, hz: number, opts?: ColliderOptions): void;
  capsuleY(commands: Commands, entity: Entity, halfHeight: number, radius: number, opts?: ColliderOptions): void;
};
