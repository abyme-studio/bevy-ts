// A prefab, entity-local event props, and spawning a prefab from a system.
import { app, type Commands, inState, type Local, type Query, type Res, SceneComponent, Time, Transform, type f32, type u8, type u32, Update, systems, type Vec3Like } from "bevy-ts";
import { Collider, RigidBody } from "bevy-ts/rapier";
import { Alive, Enemy, Explode, Game, Health } from "./systems.ts";

/** What a grunt is: Bevy's scene component. Where grunts are is a `.bsn` level (levels change),
 *  which places one as `@prefab::Grunt { @speed: 3.0, team: 1 }`, exactly as Rust's BSN does. */
export class Grunt extends SceneComponent {
  team: u8 = 0;                                                       // component field, patchable
  static scene({ at = [0, 0, 0], speed = 2 }: { at?: Vec3Like; speed?: f32 }) {   // props
    return (
      <entity name="grunt" onPointerClick={({ entity, commands }) => commands.trigger(Explode.of({ entity, radius: 3 }))}>
        <Transform translation={at} />
        <Enemy speed={speed} />
        <Health initialHp={100} max={100} />
        <Alive />
        <RigidBody value="Dynamic" />
        <Collider cuboid={[0.5, 0.5, 0.5]} />
        <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
      </entity>
    );
  }
}

/** A plain function prefab: usable inside TSX only, like a Rust scene function. */
export const Eye = ({ size = 0.2 }: { size?: f32 }) => <entity key="eye"><mesh shape="sphere" size={[size, size, size]} /></entity>;

export function countGrunts(grunts: Query<[Grunt]>) {
  if (grunts.len > 50) console.warn("too many grunts");
}

export function waves(commands: Commands, time: Res<Time>, s: Local<{ timer: f32; wave: u32 }>, alive: Query<[Enemy]>) {
  s.timer += time.delta_secs;
  if (s.timer < 5 || alive.len > 20) return;
  s.timer = 0;
  s.wave += 1;
  for (let i = 0; i < s.wave; i++) commands.spawn(<Grunt at={[i * 2 - s.wave, 0, -12]} speed={2 + s.wave * 0.5} team={1} />);
}

app.addSystems(Update, systems(waves).runIf(inState(Game.Playing)));
