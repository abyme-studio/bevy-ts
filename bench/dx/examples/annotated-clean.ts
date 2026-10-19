// F, cleaned up: a component is a class (value and type in one line, its instance type IS the
// data), params are plain types, and a param bundle is a plain interface (Rust's #[derive(SystemParam)]).
import { f32, u32, vec3, type Entity, type ReadonlyVec3, type Schema, type Vec3, type MutRef } from "bevy-ts";

declare function Component<const S extends Schema>(name: string, fields: S): { new (): MutRef<S>; readonly schema: S };
declare function Resource<const S extends Schema>(name: string, fields: S): { new (): MutRef<S>; readonly schema: S };

export class Enemy extends Component("game::Enemy", { speed: f32 }) {}
export class Player extends Component("game::Player", {}) {}
export class Alive extends Component("game::Alive", {}) {}
export class Transform extends Component("bevy::Transform", { translation: vec3, scale: vec3 }) {}
export class Time extends Resource("bevy::Time", { delta: f32, elapsed: f32 }) {}
export class Score extends Resource("game::Score", { value: u32 }) {}

// Param vocabulary, types only.
declare const mut: unique symbol;
export type Mut<T> = { readonly [mut]: T };
export type With<T> = { readonly with: T };
export type Without<T> = { readonly without: T };
type ReadView<T> = { readonly [K in keyof T]: T[K] extends Vec3 ? ReadonlyVec3 : T[K] };
type Item<D> = D extends Mut<infer T> ? T : ReadView<D>;
type Row<Ds extends readonly unknown[]> = { -readonly [I in keyof Ds]: Item<Ds[I]> };
export interface Query<Ds extends readonly unknown[], _F = never> {
  each(f: (...row: Row<Ds>) => void): void;
  single(): Row<Ds>;
  get(e: Entity): Row<Ds> | undefined;
}
export type Res<T> = ReadView<T>;
export type ResMut<T> = T;

const dir = vec3();

export function chase(
  time: Res<Time>,
  player: Query<[Transform], With<Player>>,
  enemies: Query<[Enemy, Mut<Transform>], [With<Alive>, Without<Player>]>,
) {
  const [p] = player.single();
  enemies.each((enemy, t) => {
    dir.subVectors(p.translation, t.translation).normalize();
    t.translation.addScaled(dir, enemy.speed * time.delta);
    // @ts-expect-error  read-only
    enemy.speed = 3;
    // @ts-expect-error  read Transform's vectors have no mutating methods
    p.translation.add(dir);
  });
}

// An alias and a param bundle, both resolvable without a type checker.
export type Enemies = Query<[Enemy, Mut<Transform>], With<Alive>>;
export interface Scoring { score: ResMut<Score>; time: Res<Time> }

export function scoreUp(enemies: Enemies, { score, time }: Scoring) {
  enemies.each(() => { score.value += Math.floor(time.delta); });
}
