// Type tests for delayed commands (docs/time.md, "Delayed commands"): `tsc -p test` must pass, and
// every `@ts-expect-error` must be an error.
import { Component, type Commands, type DelayedCommands, type Entity } from "bevy-ts";

export class Dummy extends Component {}

export function later(commands: Commands, e: Entity) {
  commands.delayed().secs(1).entity(e).despawn();
  const d: DelayedCommands = commands.delayed();
  const spawned: Entity = d.secs(0.5).spawn(Dummy);
  d.duration(1).delayed().secs(1).entity(spawned).insert(Dummy);
  // @ts-expect-error  a delay is a number of seconds
  d.secs("1");
  // @ts-expect-error  `DelayedCommands` only gives a delay; the commands are on what it returns
  commands.delayed().spawn(Dummy);
}
