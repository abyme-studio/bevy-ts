// The proposed system surface, type-checked with `deno check`. Every `@ts-expect-error` line
// must be an error, or the check fails.
import {
  Add, app, chain, Commands, component, EntityId, entityEvent, f32, inState, Local, message, MessageReader,
  MessageWriter, Mut, NextState, observer, On, OnEnter, Option, plugin, Query, Res, ResMut, resource, states,
  system, systemSet, tag, Time, Transform, u32, Update, vec3, FixedUpdate, entity,
} from "bevy-ts";
import { Collider, Velocity } from "bevy-ts/rapier";

// ---------- Declarations ----------

export const Enemy = component("game::Enemy", { speed: f32.default(2) });
export const Health = component("game::Health", { hp: f32.default(100), max: f32.default(100) });
export const Player = tag("game::Player");
export const Alive = tag("game::Alive");
export const Score = resource("game::Score", { value: u32 });
export const Game = states("game::Game", ["Menu", "Playing", "Paused"]);
export const Damage = message("game::Damage", { target: entity, amount: f32 });
export const Explode = entityEvent("game::Explode", { radius: f32 }, { propagate: false });

// ---------- 1. A system with two queries and a resource ----------

const toPlayer = vec3(); // scratch, reused every row: no allocation in the loop

export const chase = system(
  {
    time: Res(Time),
    player: Query([Transform], { with: [Player] }),
    enemies: Query([Enemy, Mut(Transform)], { with: [Alive], without: [Player] }),
  },
  ({ time, player, enemies }) => {
    const [target] = player.single();
    enemies.each((enemy, t) => {
      toPlayer.subVectors(target.translation, t.translation).normalize();
      t.translation.addScaled(toPlayer, enemy.speed * time.delta);
      // @ts-expect-error  Enemy is read-only in this query
      enemy.speed = 3;
      // @ts-expect-error  a vector field is a view; mutate it in place, do not reassign it
      t.translation = toPlayer;
    });
    // @ts-expect-error  Ref<Transform> has no mutating methods
    target.translation.add(toPlayer);
  },
);

// Entity, Option and Changed, Bevy-style.
export const regen = system(
  { q: Query([EntityId, Mut(Health), Option(Enemy)], { changed: [Health] }), w: MessageWriter(Damage) },
  ({ q, w }) => {
    for (const [e, h, enemy] of q) {
      h.hp = Math.min(h.max, h.hp + (enemy ? 0 : 1));
      if (h.hp <= 0) w.write({ target: e, amount: 0 });
      // @ts-expect-error  `sped` is not a field
      if (enemy) enemy.sped;
    }
  },
);

// Two queries over the same component, one writing: no ParamSet. The stage is exclusive and JS is
// single-threaded, so overlapping views alias the same column memory, as two `&mut` never could.
export const separate = system(
  { a: Query([EntityId, Mut(Transform)], { with: [Enemy] }), b: Query([EntityId, Transform], { with: [Enemy] }) },
  ({ a, b }) => {
    a.each((ea, ta) => b.each((eb, tb) => {
      if (ea !== eb && ta.translation.distance(tb.translation) < 1) ta.translation.y += 0.01;
    }));
  },
);

// @ts-expect-error  Collider is reflect tier (Arc<dyn Shape>, no Reflect): never query data
Query([Mut(Collider)]);
// Reflect tier is fine as a filter.
export const moving = Query([Mut(Velocity)], { with: [Collider] });

// ---------- 2. Ordering, run conditions, states ----------

export const spawnWave = system({ commands: Commands, n: Local({ wave: u32, timer: f32 }), time: Res(Time) },
  ({ commands, n, time }) => {
    n.timer += time.delta;
    if (n.timer < 5) return;
    n.timer = 0;
    n.wave += 1;
    for (let i = 0; i < n.wave; i++) commands.spawn(Enemy({ speed: 2 + n.wave }), Transform({ translation: [i, 0, -10] }), Alive());
  });

export const pause = system({ next: NextState(Game) }, ({ next }) => {
  next.set(Game.Paused);
  // @ts-expect-error  not a Game value
  next.set("Paused");
});

export const EnemySet = systemSet("game::Enemies");

// ---------- 3. A message and an observer ----------

export const applyDamage = system(
  { damage: MessageReader(Damage), q: Query([Mut(Health)]), score: ResMut(Score), commands: Commands },
  ({ damage, q, score, commands }) => {
    damage.each((d) => {
      const [h] = q.get(d.target) ?? [];
      if (!h) return;
      h.hp -= d.amount;
      if (h.hp <= 0) {
        score.value += 1;
        commands.trigger(Explode, d.target, { radius: 3 });
      }
    });
    // @ts-expect-error  amount is a number
    commands.write(Damage, { target: undefined as never, amount: "lots" });
  },
);

export const onExplode = observer(
  On(Explode),
  { q: Query([EntityId, Transform], { with: [Enemy] }), damage: MessageWriter(Damage), commands: Commands },
  (on, { q, damage, commands }) => {
    const [, at] = q.get(on.entity) ?? [];
    if (!at) return;
    q.each((e, t) => {
      if (t.translation.distance(at.translation) < on.event.radius) {
        const m = damage.next(); // allocation-free write
        m.target = e;
        m.amount = 50;
      }
    });
    commands.entity(on.entity).despawn();
  },
);

export const onHealthAdded = observer(Add(Health), { q: Query([Mut(Health)]) }, (on, { q }) => {
  const [h] = q.get(on.entity) ?? [];
  if (h) h.hp = h.max;
});

// ---------- 4. A plugin ----------

export const EnemyPlugin = plugin((app) => {
  app
    .initResource(Score, { value: 0 })
    .addMessage(Damage)
    .initState(Game, Game.Menu)
    .addSystems(OnEnter(Game.Playing), spawnWave)
    .addSystems(
      Update,
      chain(spawnWave, chase, separate).inSet(EnemySet).runIf(inState(Game.Playing)),
      applyDamage.after(EnemySet),
    )
    .addSystems(FixedUpdate, regen)
    .addObserver(onExplode)
    .addObserver(onHealthAdded);
});

app.addPlugins(EnemyPlugin);
