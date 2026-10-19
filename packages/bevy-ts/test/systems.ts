// Type tests for the ruled surface (from bench/dx/examples/systems.ts, without the Rapier port):
// `tsc -p test` must pass, and every `@ts-expect-error` must be an error.
import {
  type Add, app, chain, type Changed, Component, type Commands, type Entity, EntityEvent, f32, FixedUpdate,
  entity, inState, type Local, Message, type MessageReader, type MessageWriter, type Mut, type NextState, type On,
  OnEnter, type Option, type Query, type Res, type ResMut, Resource, States, systems, systemSet, Tag, Time,
  Transform, u32, Update, vec3, type With, type Without,
} from "bevy-ts";
import { GlobalTransform } from "bevy-ts/bevy";

// ---------- Declarations ----------

export class Enemy extends Component("game::Enemy", { speed: f32.default(2) }) {}
export class Health extends Component("game::Health", { hp: f32.default(100), max: f32.default(100) }) {}
export class Player extends Tag("game::Player") {}
export class Alive extends Tag("game::Alive") {}
export class Score extends Resource("game::Score", { value: u32 }) {}
export class Game extends States("game::Game", ["Menu", "Playing", "Paused"]) {}
export class Damage extends Message("game::Damage", { target: entity, amount: f32 }) {}
export class Explode extends EntityEvent("game::Explode", { radius: f32 }) {}

const dir = vec3(); // scratch, made once

// ---------- 1. A system with two queries and a resource ----------

export function chase(
  time: Res<Time>,
  player: Query<[Transform], With<Player>>,
  enemies: Query<[Enemy, Mut<Transform>], [With<Alive>, Without<Player>]>,
) {
  const [p] = player.single();
  enemies.each((enemy, t) => {
    dir.subVectors(p.translation, t.translation).normalize();
    t.translation.addScaled(dir, enemy.speed * time.delta);
    // @ts-expect-error  Enemy is read-only in this query
    enemy.speed = 3;
    // @ts-expect-error  a vector field is a view; mutate it in place, do not reassign it
    t.translation = dir;
  });
  // @ts-expect-error  a read Transform has no mutating methods
  p.translation.add(dir);
}

// Entity, Option and Changed, Bevy-style.
export function regen(q: Query<[Entity, Mut<Health>, Option<Enemy>], Changed<Health>>) {
  for (const [, h, enemy] of q) {
    h.hp = Math.min(h.max, h.hp + (enemy ? 0 : 1));
    // @ts-expect-error  `sped` is not a field
    if (enemy) enemy.sped;
  }
}

// Two overlapping queries, one writing: no ParamSet. The stage is exclusive and JS is
// single-threaded, so both views alias the same column memory.
export function separate(
  a: Query<[Entity, Mut<Transform>], With<Enemy>>,
  b: Query<[Entity, Transform], With<Enemy>>,
) {
  a.each((ea, ta) => b.each((eb, tb) => {
    if (ea !== eb && ta.translation.distance(tb.translation) < 1) ta.translation.y += 0.01;
  }));
}

// Reflect tier is a filter, never query data.
export function grounded(moving: Query<[Mut<Transform>], With<GlobalTransform>>) {
  moving.each((t) => t.translation.scale(0.99));
}
// @ts-expect-error  GlobalTransform has no native layout
export function bad(q: Query<[Mut<GlobalTransform>]>) { q.each((g) => g.translation); }

// ---------- 2. Ordering, run conditions, states ----------

export function spawnWave(commands: Commands, wave: Local<{ n: u32 }>) {
  wave.n += 1;
  for (let i = 0; i < wave.n; i++) {
    commands.spawn(new Enemy({ speed: 2 + wave.n }), new Transform({ translation: [i, 0, -10] }), new Alive());
  }
}

export function pause(next: NextState<Game>) {
  next.set(Game.Paused);
  // @ts-expect-error  not a Game value
  next.set("Paused");
}

export const EnemySet = systemSet("game::Enemies");

// ---------- 3. A message and an observer ----------

export function applyDamage(
  damage: MessageReader<Damage>,
  q: Query<[Mut<Health>]>,
  score: ResMut<Score>,
  commands: Commands,
) {
  damage.each((d) => {
    const [h] = q.get(d.target) ?? [];
    if (!h) return;
    h.hp -= d.amount;
    if (h.hp <= 0) {
      score.value += 1;
      commands.trigger(d.target, new Explode({ radius: 3 }));
    }
  });
  // @ts-expect-error  a message is a Message instance, not a plain object
  commands.write({ amount: 1 });
}

export function onExplode(
  explode: On<Explode>,
  q: Query<[Entity, Transform], With<Enemy>>,
  damage: MessageWriter<Damage>,
  commands: Commands,
) {
  const [, at] = q.get(explode.entity) ?? [];
  if (!at) return;
  q.each((e, t) => {
    if (t.translation.distance(at.translation) < explode.event.radius) damage.write({ target: e, amount: 50 });
  });
  commands.entity(explode.entity).despawn();
}

export function onHealthAdded(add: On<Add<Health>>, q: Query<[Mut<Health>]>) {
  const [h] = q.get(add.entity) ?? [];
  if (h) h.hp = h.max;
}

// ---------- 4. A plugin ----------

export function enemyPlugin(app: import("bevy-ts").App) {
  app
    .insertResource(new Score({ value: 0 }))
    .addMessage(Damage)
    .initState(Game.Menu)
    .addSystems(OnEnter(Game.Playing), spawnWave)
    .addSystems(
      Update,
      chain(spawnWave, chase, separate).inSet(EnemySet).runIf(inState(Game.Playing)),
      systems(applyDamage).after(EnemySet),
      systems(pause).runIf(inState(Game.Playing)),
    )
    .addSystems(FixedUpdate, regen, grounded)
    .addObserver(onExplode)
    .addObserver(onHealthAdded);
}

app.addPlugins(enemyPlugin);
