// Type tests for the ruled surface (from bench/dx/examples/systems.ts, without the Rapier port):
// `tsc -p test` must pass, and every `@ts-expect-error` must be an error.
import {
  type Add, app, chain, type Changed, Component, type Commands, type Entity, EntityEvent, type f32, FixedUpdate,
  inState, type Local, Message, type MessageReader, type MessageWriter, type Mut, type NextState, type On,
  OnEnter, type Option, type Query, type ReadView, type Res, type ResMut, Resource, SceneComponent, States, systems, SystemSet, Time,
  Transform, type u8, type u32, Update, vec3, type Vec3Like, type With, type Without,
} from "bevy-ts";
import { GlobalTransform } from "bevy-ts/bevy";

// ---------- Declarations ----------

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
/** A placeable prefab: `team` is a field, `at` and `speed` are scene props. */
export class Grunt extends SceneComponent {
  team: u8 = 0;
  static scene({ at = [0, 0, 0], speed = 2 }: { at?: Vec3Like; speed?: f32 }) {
    void at, speed;
    return null;
  }
}

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
export function countGrunts(grunts: Query<[Grunt]>) {
  if (grunts.len > 50) console.warn(`too many ${Grunt.componentName}`);
}

export function grounded(moving: Query<[Mut<Transform>], With<GlobalTransform>>) {
  moving.each((t) => t.translation.scale(0.99));
}
// @ts-expect-error  GlobalTransform has no native layout
export function bad(q: Query<[Mut<GlobalTransform>]>) { q.each((g) => g.translation); }

// ---------- 2. Ordering, run conditions, states ----------

export function spawnWave(commands: Commands, wave: Local<{ n: u32 }>) {
  wave.n += 1;
  for (let i = 0; i < wave.n; i++) {
    commands.spawn(Enemy.of({ speed: 2 + wave.n }), Transform.of({ translation: [i, 0, -10] }), Alive);
  }
}

export function pause(next: NextState<Game>) {
  next.set(Game.Paused);
  // @ts-expect-error  not a Game value
  next.set("Paused");
}

export class EnemySet extends SystemSet {}

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
      commands.trigger(Explode.of({ entity: d.target, radius: 3 }));
    }
  });
  // @ts-expect-error  a message is a Message value, not a plain object
  commands.write({ amount: 1 });
  // @ts-expect-error  `sped` is not a field of Enemy
  Enemy.of({ sped: 1 });
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
    if (t.translation.distance(at.translation) < explode.radius) damage.write({ target: e, amount: 50 });
  });
  commands.entity(explode.entity).despawn();
}

export function onHealthAdded(add: On<Add<Health>>, q: Query<[Mut<Health>]>) {
  const [h] = q.get(add.entity) ?? [];
  if (h) h.hp = h.max;
}

// ---------- 4. Optional resources ----------

export function bonus(score: Option<Res<Score>>, best: Option<ResMut<Score>>, q: Query<[Option<Mut<Health>>]>) {
  const view: ReadView<Score> | undefined = score;
  void view;
  // @ts-expect-error  an absent resource is undefined
  void score.value;
  if (score) {
    // @ts-expect-error  Res is read-only
    score.value = 1;
  }
  if (best) best.value += 1;
  for (const [h] of q) {
    // @ts-expect-error  Option<Mut<C>> query data may be undefined
    h.hp = 0;
    if (h) h.hp = 0;
  }
}

// ---------- 5. A plugin ----------

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
    .addSystems(FixedUpdate, regen, grounded)
    .addObserver(onExplode)
    .addObserver(onHealthAdded);
}

app.addPlugins(enemyPlugin);
