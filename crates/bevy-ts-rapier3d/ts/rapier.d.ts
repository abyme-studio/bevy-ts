import type { Commands, Component, Entity, Message, Vec2, Vec3 } from "bevy-ts";
/** Rapier's `CollisionEvent`, flattened: read it with `MessageReader<CollisionEvent>` after `app.addMessage(CollisionEvent)`. */
export declare class CollisionEvent extends Message {
  static readonly path: "bevy_rapier3d::pipeline::events::CollisionEvent";
  kind: "Started" | "Stopped";
  a: Entity;
  b: Entity;
  sensor: boolean;
}
export interface ColliderOptions { sensor?: boolean }
/** `Collider`: as a TSX element `<Collider cuboid={[0.5, 0.5, 0.5]} />` (one of ball, cuboid,
 * capsule_y); as commands `Collider.cuboid(commands, e, hx, hy, hz)`. Rapier's `Collider` has no
 * `Reflect`, so the port builds it from this plain-data stand-in. */
export declare class Collider extends Component {
  static readonly path: "bevy_ts_rapier3d::ColliderShape";
  ball: number;
  readonly cuboid: Vec3;
  readonly capsule_y: Vec2;
  sensor: boolean;
  static ball(commands: Commands, entity: Entity, radius: number, opts?: ColliderOptions): void;
  static cuboid(commands: Commands, entity: Entity, hx: number, hy: number, hz: number, opts?: ColliderOptions): void;
  static capsuleY(commands: Commands, entity: Entity, halfHeight: number, radius: number, opts?: ColliderOptions): void;
}
