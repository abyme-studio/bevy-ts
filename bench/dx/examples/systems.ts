// The ruled system surface (operator, 2026-10-20): Rust-style annotations read by a syntax-only
// pass in oxc. Type-checked with `deno check`; every `@ts-expect-error` must be an error.
// `deno run -A bench/dx/read-annotations.ts bench/dx/examples/systems.ts` prints what the engine reads.
import {
  type Add, app, chain, type Changed, Component, type Commands, type Entity, EntityEvent, type f32, FixedUpdate,
  inState, type Local, Message, type MessageReader, type MessageWriter, type Mut, type NextState, type On,
  OnEnter, type Option, type Query, type Res, type ResMut, Resource, States, systems, systemSet, Time,
  Transform, type u32, Update, vec3, type With, type Without,
} from "bevy-ts";
import { Collider, Velocity } from "bevy-ts/rapier";

// ---------- Declarations: plain classes; the compile pass reads their fields ----------

export class Enemy extends Component { speed: f32 = 2 }
export class Health extends Component { hp: f32 = 100; max: f32 = 100 }
export class Player extends Component {}
export class Alive extends Component {}
export class Score extends Resource { value: u32 = 0 }
export class Game extends States("Menu", "Playing", "Paused") {}
export class Damage extends Message { declare target: Entity; declare amount: f32 }
export class Explode extends EntityEvent { declare radius: f32 }
/** Pinned name: survives renaming the file or the class. */
export class Shield extends Component { static path = "game::Shield"; strength: f32 = 0 }

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
    t.translation.addScaled(dir, enemy.speed * time.delta_secs);
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
export function drag(moving: Query<[Mut<Velocity>], With<Collider>>) {
  moving.each((v) => v.linear.scale(0.99));
}
// @ts-expect-error  Collider is reflect tier (Arc<dyn Shape>, no Reflect)
export function bad(q: Query<[Mut<Collider>]>) { q.each((c) => c.ball); }

// ---------- 2. Ordering, run conditions, states ----------

export function spawnWave(commands: Commands, wave: Local<{ n: u32 }>) {
  wave.n += 1;
  for (let i = 0; i < wave.n; i++) {
    commands.spawn(Enemy.of({ speed: 2 + wave.n }), Transform.of({ translation: [i, 0, -10] }), Alive.of());
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
      commands.trigger(d.target, Explode.of({ radius: 3 }));
    }
  });
  // @ts-expect-error  amount is a number
  commands.write(Damage.of({ amount: "lots" }));
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
    .insertResource(Score.of({ value: 0 }))
    .addMessage(Damage)
    .initState(Game.Menu)
    .addSystems(OnEnter(Game.Playing), spawnWave)
    .addSystems(
      Update,
      chain(spawnWave, chase, separate).inSet(EnemySet).runIf(inState(Game.Playing)),
      systems(applyDamage).after(EnemySet),
      systems(pause).runIf(inState(Game.Playing)),
    )
    .addSystems(FixedUpdate, regen, drag)
    .addObserver(onExplode)
    .addObserver(onHealthAdded);
}

app.addPlugins(enemyPlugin);
