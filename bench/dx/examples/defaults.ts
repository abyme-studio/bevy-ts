// Params in the signature, as Rust has them: default parameters declare, the engine passes values.
import { Mut, query, res, Time, Transform, vec3, type System } from "bevy-ts";
import { Alive, Enemy, Player } from "./systems.ts";

declare function system(f: (...args: never[]) => void): System;
const dir = vec3();

export const chase = system((
  time = res(Time),
  player = query(Transform).with(Player),
  enemies = query(Enemy, Mut(Transform)).with(Alive).without(Player),
) => {
  const [p] = player.single();
  enemies.each((enemy, t) => {
    dir.subVectors(p.translation, t.translation).normalize();
    t.translation.addScaled(dir, enemy.speed * time.delta);
    // @ts-expect-error  read-only still enforced
    enemy.speed = 3;
  });
});
