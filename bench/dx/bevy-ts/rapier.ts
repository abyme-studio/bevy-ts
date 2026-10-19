// What `bevy-ts/rapier` would ship: the TS half of a curated port of bevy_rapier3d 0.37.
// Data types are generated from the engine's registry (layout tier where every leaf is a known
// POD type, offsets from reflection); constructors, sets and functions are the curated part.
import { bool, Component, entity, enumOf, f32, Message, ReflectComponent, vec3 } from "./mod.ts";
import type { Entity, ReadonlyVec3, SystemSet } from "./mod.ts";

/** `Velocity { pub linear: Vec3, pub angular: Vec3 }`: not repr(C); a column all the same. */
export declare class Velocity extends Component("bevy_rapier3d::dynamics::rigid_body::Velocity", { linear: vec3, angular: vec3 }) {}
export declare class ExternalImpulse extends Component("bevy_rapier3d::dynamics::rigid_body::ExternalImpulse", { impulse: vec3, torque_impulse: vec3 }) {}
/** A fieldless enum: one byte, values read from this build at startup. */
export declare class RigidBody extends Component("bevy_rapier3d::dynamics::rigid_body::RigidBody", {
  value: enumOf("Dynamic", "Fixed", "KinematicPositionBased", "KinematicVelocityBased"),
}) {}
/** `Collider` holds an `Arc<dyn Shape>` and has no `Reflect`: a curated constructor, reflect tier. */
export declare class Collider extends ReflectComponent("bevy_rapier3d::geometry::collider::Collider", { ball: f32, cuboid: vec3, capsuleY: vec3 }) {}
/** `enum CollisionEvent { Started(Entity, Entity, Flags), Stopped(..) }`, flattened by the port. */
export declare class CollisionEvent extends Message("bevy_rapier3d::pipeline::events::CollisionEvent", {
  kind: enumOf("Started", "Stopped"), a: entity, b: entity, sensor: bool,
}) {}
/** `PhysicsSet`, exposed as named sets with before/after slots (native decision 7). */
export declare const PhysicsSet: { readonly SyncBackend: SystemSet; readonly StepSimulation: SystemSet; readonly Writeback: SystemSet };

export interface RayHit { readonly entity: Entity; readonly toi: number; readonly point: ReadonlyVec3 }
/** `ReadRapierContext`, a system param: read-only world access declared in Rust. */
export interface ReadRapierContext {
  /** One host call. Returns a view reused by the next call. */
  castRay(origin: ReadonlyVec3, dir: ReadonlyVec3, maxToi: number, solid: boolean, filter?: { exclude?: Entity }): RayHit | undefined;
  /** One host call for many rays (native decision 8). */
  castRays(rays: Float32Array, maxToi: number, out: { entity: Uint32Array; toi: Float32Array }): number;
}
