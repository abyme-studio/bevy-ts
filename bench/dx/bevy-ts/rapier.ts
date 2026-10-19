// What `bevy-ts/rapier` would ship: the TS half of a curated port of bevy_rapier3d 0.37.
// Data types are generated from the engine's registry (layout tier where leaves are plain data,
// offsets from reflection); constructors, sets and exported params are the curated part.
import { Component, Message, ReflectComponent } from "./mod.ts";
import type { Entity, f32, ReadonlyVec3, SystemSet, Vec3 } from "./mod.ts";

/** `Velocity { pub linear: Vec3, pub angular: Vec3 }`: not repr(C); a column all the same. */
export declare class Velocity extends Component {
  static readonly path: "bevy_rapier3d::dynamics::rigid_body::Velocity";
  declare readonly linear: Vec3;
  declare readonly angular: Vec3;
}
export declare class ExternalImpulse extends Component {
  static readonly path: "bevy_rapier3d::dynamics::rigid_body::ExternalImpulse";
  declare readonly impulse: Vec3;
  declare readonly torque_impulse: Vec3;
}
/** A fieldless enum: one byte, values read from this build at startup. */
export declare class RigidBody extends Component {
  static readonly path: "bevy_rapier3d::dynamics::rigid_body::RigidBody";
  declare value: "Dynamic" | "Fixed" | "KinematicPositionBased" | "KinematicVelocityBased";
}
/** `Collider` holds an `Arc<dyn Shape>` and has no `Reflect`: a curated constructor, reflect tier. */
export declare class Collider extends ReflectComponent {
  static readonly path: "bevy_rapier3d::geometry::collider::Collider";
  declare ball: f32;
  declare cuboid: Vec3;
  declare capsuleY: Vec3;
}
/** `enum CollisionEvent { Started(Entity, Entity, Flags), Stopped(..) }`, flattened by the port. */
export declare class CollisionEvent extends Message {
  static readonly path: "bevy_rapier3d::pipeline::events::CollisionEvent";
  declare kind: "Started" | "Stopped";
  declare a: Entity;
  declare b: Entity;
  declare sensor: boolean;
}
/** `PhysicsSet`, exposed as named sets (native decision 7). */
export declare const PhysicsSet: { readonly SyncBackend: SystemSet; readonly StepSimulation: SystemSet; readonly Writeback: SystemSet };

export interface RayHit { readonly entity: Entity; readonly toi: number; readonly point: ReadonlyVec3 }
/** `ReadRapierContext`, exported by the port with `#[bevy_ts::export]` (native decision 8). */
export interface ReadRapierContext {
  /** One fast host call. Returns a view reused by the next call. */
  castRay(origin: ReadonlyVec3, dir: ReadonlyVec3, maxToi: number, solid: boolean, filter?: { excludeRigidBody?: Entity }): RayHit | undefined;
  /** One host call for many rays. */
  castRays(rays: Float32Array, maxToi: number, out: { entity: Uint32Array; toi: Float32Array }): number;
}
