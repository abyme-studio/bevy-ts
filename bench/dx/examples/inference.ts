// Exact inferred row types for an annotated query, asserted at compile time.
import type { Entity, Mut, Option, Query, Quat, ReadonlyVec3, Vec3, With } from "bevy-ts";
import type { Transform } from "bevy-ts";
import type { Enemy, Health, Alive } from "./systems.ts";

type Equals<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
type Assert<T extends true> = T;
type RowOf<Q> = Q extends Iterable<infer R> ? R : never;
type IsReadonly<T, K extends keyof T> = Equals<{ [P in K]: T[P] }, { -readonly [P in K]: T[P] }> extends true ? false : true;

type R = RowOf<Query<[Entity, Enemy, Mut<Transform>, Option<Health>], With<Alive>>>;

export type Checks = [
  Assert<Equals<R["length"], 4>>,
  Assert<Equals<R[0], Entity>>,
  Assert<Equals<R[1]["speed"], number>>,
  Assert<IsReadonly<R[1], "speed">>,
  Assert<Equals<R[2]["translation"], Vec3>>,
  Assert<Equals<R[2]["rotation"], Quat>>,
  Assert<Equals<NonNullable<R[3]>["hp"], number>>,
  Assert<Equals<undefined extends R[3] ? true : false, true>>,
  Assert<Equals<RowOf<Query<[Transform]>>[0]["translation"], ReadonlyVec3>>,
];
