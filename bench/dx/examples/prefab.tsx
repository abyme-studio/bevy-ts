// A prefab, entity-local event props, and spawning a prefab from a system.
import { app, type Commands, inState, type Local, type Query, type Res, Time, Transform, type f32, type u32, Update, systems } from "bevy-ts";
import { Collider, RigidBody } from "bevy-ts/rapier";
import { Alive, Enemy, Explode, Game, Health } from "./systems.ts";

/** What a grunt is. Where grunts are is a `.bsn` level (levels change). */
export const Grunt = ({ at, speed = 2 }: { at: readonly [number, number, number]; speed?: number }) => (
  <entity name="grunt" onPointerClick={({ entity, commands }) => commands.trigger(entity, Explode.of({ radius: 3 }))}>
    <Transform translation={at} />
    <Enemy speed={speed} />
    <Health initialHp={100} max={100} />
    <Alive />
    <RigidBody value="Dynamic" />
    <Collider cuboid={[0.5, 0.5, 0.5]} />
    <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
  </entity>
);

export function waves(commands: Commands, time: Res<Time>, s: Local<{ timer: f32; wave: u32 }>, alive: Query<[Enemy]>) {
  s.timer += time.delta_secs;
  if (s.timer < 5 || alive.len > 20) return;
  s.timer = 0;
  s.wave += 1;
  for (let i = 0; i < s.wave; i++) commands.spawn(<Grunt at={[i * 2 - s.wave, 0, -12]} speed={2 + s.wave * 0.5} />);
}

app.addSystems(Update, systems(waves).runIf(inState(Game.Playing)));
