// Exact inferred types, asserted at compile time.
import { EntityId, Mut, Option, Query, Transform, type Entity, type Param, type Query as Q, type Vec3, type ReadonlyVec3, type Quat } from "bevy-ts";
import { Enemy, Health } from "./systems.ts";

type Equals<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;
type RowOf<P> = P extends Param<Q<infer R>> ? R : never;
type IsReadonly<T, K extends keyof T> = Equals<{ [P in K]: T[P] }, { -readonly [P in K]: T[P] }> extends true ? false : true;

const q = Query([EntityId, Enemy, Mut(Transform), Option(Health)]);
type R = RowOf<typeof q>;

export type Checks = [
  Assert<Equals<R["length"], 4>>,
  Assert<Equals<R[0], Entity>>,
  Assert<Equals<R[1]["speed"], number>>,
  Assert<IsReadonly<R[1], "speed">>,
  Assert<Equals<R[2]["translation"], Vec3>>,
  Assert<Equals<R[2]["rotation"], Quat>>,
  Assert<Equals<NonNullable<R[3]>["hp"], number>>,
  Assert<Equals<undefined extends R[3] ? true : false, true>>,
  Assert<Equals<RowOf<ReturnType<typeof Query<[typeof Transform]>>>[0]["translation"], ReadonlyVec3>>,
];
