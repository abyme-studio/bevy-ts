// Type tests for Rust messages and events (openspec rust-events): `tsc -p test` must pass, and
// every `@ts-expect-error` must be an error.
import { app, type Commands, type Entity, EntityEvent, Event, type f32, type MessageReader, type MessageWriter, type On, rustEntityEvent, rustEvent, rustMessage, type u32, Update } from "bevy-ts";
import { AssetEvent, GamepadRumbleRequest, type Image, KeyboardInput, PointerClick, ValueChange } from "bevy-ts/bevy";

export class Explode extends EntityEvent { declare radius: f32 }
export class Ping extends Event { declare n: f32 }

// A struct message reads as its fields; an enum one as its value in Bevy's shape.
export function keys(r: MessageReader<KeyboardInput>, rumble: MessageWriter<GamepadRumbleRequest>, pad: Entity) {
  r.each((k) => {
    const code: string | object = k.key_code;
    void code;
    // @ts-expect-error  a read message is read-only
    k.repeat = true;
  });
  rumble.write(GamepadRumbleRequest.of({ Add: { gamepad: pad, duration: 0.5, intensity: { strong_motor: 1, weak_motor: 0.25 } } }));
  // @ts-expect-error  not a GamepadRumbleRequest variant
  rumble.write(GamepadRumbleRequest.of({ Shake: {} }));
}

// A generic by its argument.
export function images(r: MessageReader<AssetEvent<Image>>) {
  for (const e of r) void e;
}

// On<E> of a Rust event: its fields, its target, the whole value as `event`.
export function clicked(click: On<PointerClick>) {
  const first: Entity = click.original;
  const own: Entity = click.event.entity;
  const n: number = click.count;
  click.propagate(false);
  void first, own, n;
}

// `event` on a TS event too.
export function exploded(e: On<Explode>) {
  const r: number = e.event.radius;
  const target: Entity = e.event.entity;
  void r, target;
}

export function changed(v: On<ValueChange<boolean>>) {
  void v.event;
}

export function onPing(p: On<Ping>) {
  void p.n;
}

// A project's own Rust types: a class extending the factory is a type and a value, its fields typed.
export class Scored extends rustMessage("game_native::Scored") { declare points: u32 }
export class RoundOver extends rustEvent("game_native::RoundOver") { declare round: u32 }
export class Poked extends rustEntityEvent("game_native::Poked") { declare by: Entity }
export function score(r: MessageReader<Scored>, w: MessageWriter<Scored>) {
  r.each((s) => {
    const n: number = s.points;
    void n;
  });
  w.write(Scored.of({ points: 5 }));
  w.write({ points: 1 });
  const s: Scored = Scored.of({ points: 2 });
  void s;
  // @ts-expect-error  Scored has no field `goals`
  w.write(Scored.of({ goals: 1 }));
}
export function over(o: On<RoundOver>) {
  const r: number = o.round;
  void r;
}
export function poked(p: On<Poked>) {
  const by: Entity = p.by;
  const target: Entity = p.event.entity;
  void by, target;
}

// `observe` takes an observer of an entity event or a lifecycle, never a global event.
export function attach(c: Commands, e: Entity) {
  c.entity(e).observe(exploded);
  c.entity(e).observe(clicked);
  // @ts-expect-error  Ping is a global event: no entity triggers it
  c.entity(e).observe(onPing);
}

app.addSystems(Update, keys, score).addObserver(clicked).addObserver(over);
export function attachPoked(c: Commands, e: Entity) {
  c.entity(e).observe(poked);
}
