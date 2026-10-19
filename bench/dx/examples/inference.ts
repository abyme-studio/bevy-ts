// Exact inferred types, asserted at compile time.
import { EntityId, Mut, Option, query, Transform, type Entity, type Quat, type ReadonlyVec3, type Vec3 } from "bevy-ts";
import { Enemy, Health } from "./systems.ts";

type Equals<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;
type RowOf<Q> = Q extends Iterable<infer R> ? R : never;
type IsReadonly<T, K extends keyof T> = Equals<{ [P in K]: T[P] }, { -readonly [P in K]: T[P] }> extends true ? false : true;

declare const q: ReturnType<typeof query<[typeof EntityId, typeof Enemy, ReturnType<typeof Mut<typeof Transform>>, ReturnType<typeof Option<typeof Health>>]>>;
const q2 = query(EntityId, Enemy, Mut(Transform), Option(Health)).with(Health);
type R = RowOf<typeof q2>;
type R0 = RowOf<typeof q>;

export type Checks = [
  Assert<Equals<R, R0>>,
  Assert<Equals<R["length"], 4>>,
  Assert<Equals<R[0], Entity>>,
  Assert<Equals<R[1]["speed"], number>>,
  Assert<IsReadonly<R[1], "speed">>,
  Assert<Equals<R[2]["translation"], Vec3>>,
  Assert<Equals<R[2]["rotation"], Quat>>,
  Assert<Equals<NonNullable<R[3]>["hp"], number>>,
  Assert<Equals<undefined extends R[3] ? true : false, true>>,
  Assert<Equals<RowOf<ReturnType<typeof query<[typeof Transform]>>>[0]["translation"], ReadonlyVec3>>,
];
