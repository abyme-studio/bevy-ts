// A prefab, entity-local event props, and spawning a prefab from a system.
import { app, commands, f32, inState, local, query, res, system, Time, Transform, u32, Update } from "bevy-ts";
import { Collider, RigidBody } from "bevy-ts/rapier";
import { Alive, Enemy, Explode, Game, Health } from "./systems.ts";

/** What a grunt is. Where grunts are is a `.bsn` level (levels change). */
export const Grunt = ({ at, speed = 2 }: { at: readonly [number, number, number]; speed?: number }) => (
  <entity
    name="grunt"
    onPointerClick={(_, { entity, commands }) => commands.trigger(Explode, entity, { radius: 3 })}
  >
    <Transform translation={at} />
    <Enemy speed={speed} />
    <Health initialHp={100} max={100} />
    <Alive />
    <RigidBody value="Dynamic" />
    <Collider cuboid={[0.5, 0.5, 0.5]} />
    <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
  </entity>
);

export const waves = system(() => {
  const cmd = commands();
  const time = res(Time);
  const s = local({ timer: f32, wave: u32 });
  const alive = query(Enemy);
  return () => {
    s.timer += time.delta;
    if (s.timer < 5 || alive.len > 20) return;
    s.timer = 0;
    s.wave += 1;
    for (let i = 0; i < s.wave; i++) cmd.spawn(<Grunt at={[i * 2 - s.wave, 0, -12]} speed={2 + s.wave * 0.5} />);
  };
});

app.addSystems(Update, waves.runIf(inState(Game.Playing)));
