// `bevy-ts/rapier`: the TS half of the rapier3d port. Served by the engine; the classes here are
// declared by every generation, as a project's own classes are.
import { Message } from "bevy-ts";

/** Rapier's `CollisionEvent`, an enum with data in Rust, flattened to one record. */
export class CollisionEvent extends Message {
  static {
    __bevyts.data(this, {
      kind: "message",
      name: "bevy_rapier3d::pipeline::events::CollisionEvent",
      fields: [
        { name: "kind", type: { kind: "enum", variants: ["Started", "Stopped"] } },
        { name: "a", type: { kind: "entity" } },
        { name: "b", type: { kind: "entity" } },
        { name: "sensor", type: { kind: "bool" } },
      ],
    });
  }
}

/** `Collider` has no `Reflect`: these constructors insert one through the port's command. */
export const Collider = {
  ball(commands, entity, radius, opts) {
    commands.custom("rapier/collider", { entity, ball: radius, sensor: !!(opts && opts.sensor) });
  },
  cuboid(commands, entity, hx, hy, hz, opts) {
    commands.custom("rapier/collider", { entity, cuboid: [hx, hy, hz], sensor: !!(opts && opts.sensor) });
  },
  capsuleY(commands, entity, halfHeight, radius, opts) {
    commands.custom("rapier/collider", { entity, capsuleY: [halfHeight, radius], sensor: !!(opts && opts.sensor) });
  },
};
