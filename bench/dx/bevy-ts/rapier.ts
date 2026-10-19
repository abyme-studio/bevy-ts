// What `bevy-ts/rapier` would ship: the TS half of a curated port of bevy_rapier3d 0.37.
// Data types are generated from the engine's registry (layout tier where every leaf is a known
// POD type, offsets from offset_of! or reflection); the constructors, sets and functions are
// the curated part. Types only.
import type {
  ComponentHandle, Entity, Field, MessageType, Param, Plugin, ReadonlyVec3, SystemSet,
} from "./mod.ts";
import { bool, entity, f32, vec3 } from "./mod.ts";

/** `Velocity { pub linear: Vec3, pub angular: Vec3 }`: not repr(C); offsets from offset_of!. */
export declare const Velocity: ComponentHandle<"bevy_rapier3d::dynamics::rigid_body::Velocity", {
  linear: typeof vec3;
  angular: typeof vec3;
}, "layout">;
export declare const ExternalImpulse: ComponentHandle<"bevy_rapier3d::dynamics::rigid_body::ExternalImpulse", {
  impulse: typeof vec3;
  torque_impulse: typeof vec3;
}, "layout">;
export declare const GravityScale: ComponentHandle<"bevy_rapier3d::dynamics::rigid_body::GravityScale", {
  0: typeof f32;
}, "layout">;
/** A fieldless enum: one byte, values read from this build at startup. */
export declare const RigidBody: ComponentHandle<"bevy_rapier3d::dynamics::rigid_body::RigidBody", {
  value: Field<"enum", "Dynamic" | "Fixed" | "KinematicPositionBased" | "KinematicVelocityBased">;
}, "layout">;
/** `Collider` holds an `Arc<dyn Shape>` and has no `Reflect`: a curated constructor, reflect tier. */
export declare const Collider: ComponentHandle<"bevy_rapier3d::geometry::collider::Collider", {
  ball: typeof f32;
  cuboid: typeof vec3;
  capsuleY: typeof vec3;
}, "reflect">;
/** `ActiveEvents(u32)`, private field: reachable through reflection only. */
export declare const ActiveEvents: ComponentHandle<"bevy_rapier3d::geometry::collider::ActiveEvents", {
  collisionEvents: typeof bool;
  contactForceEvents: typeof bool;
}, "reflect">;

/** `enum CollisionEvent { Started(Entity, Entity, Flags), Stopped(..) }`, flattened by the port. */
export declare const CollisionEvent: MessageType<"bevy_rapier3d::pipeline::events::CollisionEvent", {
  kind: Field<"enum", "Started" | "Stopped">;
  a: typeof entity;
  b: typeof entity;
  sensor: typeof bool;
}>;

/** `PhysicsSet`, exposed as named sets with before/after slots (native decision 7). */
export declare const PhysicsSet: {
  readonly SyncBackend: SystemSet;
  readonly StepSimulation: SystemSet;
  readonly Writeback: SystemSet;
};

export interface RayHit {
  readonly entity: Entity;
  readonly toi: number;
  readonly point: ReadonlyVec3;
}
export interface RapierContext {
  /** One host call. Returns a view reused by the next call. */
  castRay(origin: ReadonlyVec3, dir: ReadonlyVec3, maxToi: number, solid: boolean, filter?: { exclude?: Entity }): RayHit | undefined;
  /** One host call for many rays (native decision 8). */
  castRays(rays: Float32Array, maxToi: number, out: { entity: Uint32Array; toi: Float32Array }): number;
}
/** `ReadRapierContext`, read-only world access declared in Rust. */
export declare const RapierContext: Param<RapierContext>;

/** `RapierPhysicsPlugin::<NoUserData>::default()`, already added by the port's Rust half. */
export declare const RapierPhysics: Plugin;

