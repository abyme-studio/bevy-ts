// Params as type annotations, Rust-style. tsc checks them as plain types; the engine's compile
// step reads the annotations (syntax only) and emits the declarations.
import { component, f32, tag, Transform as TransformValue, vec3, type ComponentType, type LayoutComponent, type MutRef, type Ref, type Entity } from "bevy-ts";

// A component is a value and a type of the same name: one line each.
export const Enemy = component("game::Enemy", { speed: f32.default(2) }); export type Enemy = typeof Enemy;
export const Player = tag("game::Player"); export type Player = typeof Player;
export const Alive = tag("game::Alive"); export type Alive = typeof Alive;
type Transform = typeof TransformValue;
type TimeRes = { readonly delta: number; readonly elapsed: number };

// Type-level param vocabulary.
declare const mutBrand: unique symbol;
type Mut<C extends LayoutComponent> = { readonly [mutBrand]: C };
type With<C extends ComponentType> = { readonly with: C };
type Without<C extends ComponentType> = { readonly without: C };
type Item<D> = D extends Mut<infer C> ? MutRef<C["schema"]> : D extends LayoutComponent ? Ref<D["schema"]> : never;
type Row<Ds extends readonly unknown[]> = { -readonly [I in keyof Ds]: Item<Ds[I]> };
interface Query<Ds extends readonly unknown[], _F = never> {
  each(f: (...row: Row<Ds>) => void): void;
  single(): Row<Ds>;
  get(e: Entity): Row<Ds> | undefined;
}
type Res<T> = T;

const dir = vec3();

export function chase(
  time: Res<TimeRes>,
  player: Query<[Transform], With<Player>>,
  enemies: Query<[Enemy, Mut<Transform>], [With<Alive>, Without<Player>]>,
) {
  const [p] = player.single();
  enemies.each((enemy, t) => {
    dir.subVectors(p.translation, t.translation).normalize();
    t.translation.addScaled(dir, enemy.speed * time.delta);
    // @ts-expect-error  read-only still enforced
    enemy.speed = 3;
  });
}
