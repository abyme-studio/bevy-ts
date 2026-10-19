// The proposed system surface in the setup form, type-checked with `deno check`. Every
// `@ts-expect-error` line must be an error, or the check fails.
import {
  Add, app, chain, commands, component, EntityId, entity, entityEvent, f32, FixedUpdate, inState, local,
  message, Mut, nextState, observer, On, OnEnter, Option, plugin, query, reader, res, resMut, resource,
  states, system, systemSet, tag, Time, Transform, u32, Update, vec3, writer,
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
export const Explode = entityEvent("game::Explode", { radius: f32 });

// ---------- 1. A system with two queries and a resource ----------

export const chase = system(() => {
  const time = res(Time);
  const player = query(Transform).with(Player);
  const enemies = query(Enemy, Mut(Transform)).with(Alive).without(Player);
  const dir = vec3(); // scratch, made once

  return () => {
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
  };
});

// Entity, Option and changed(), Bevy-style.
export const regen = system(() => {
  const q = query(EntityId, Mut(Health), Option(Enemy)).changed(Health);
  const damage = writer(Damage);
  return () => {
    for (const [e, h, enemy] of q) {
      h.hp = Math.min(h.max, h.hp + (enemy ? 0 : 1));
      if (h.hp <= 0) damage.write({ target: e, amount: 0 });
      // @ts-expect-error  `sped` is not a field
      if (enemy) enemy.sped;
    }
  };
});

// Two overlapping queries, one writing: no ParamSet. The stage is exclusive and JS is
// single-threaded, so both views alias the same column memory.
export const separate = system(() => {
  const a = query(EntityId, Mut(Transform)).with(Enemy);
  const b = query(EntityId, Transform).with(Enemy);
  return () => {
    a.each((ea, ta) => b.each((eb, tb) => {
      if (ea !== eb && ta.translation.distance(tb.translation) < 1) ta.translation.y += 0.01;
    }));
  };
});

export const physicsOnly = system(() => {
  // @ts-expect-error  Collider is reflect tier (Arc<dyn Shape>, no Reflect): never query data
  query(Mut(Collider));
  // Reflect tier is fine as a filter.
  const moving = query(Mut(Velocity)).with(Collider);
  return () => moving.each((v) => v.linear.scale(0.99));
});

// ---------- 2. Ordering, run conditions, states ----------

export const spawnWave = system(() => {
  const cmd = commands();
  const s = local({ wave: u32 });
  return () => {
    s.wave += 1;
    for (let i = 0; i < s.wave; i++) cmd.spawn(Enemy({ speed: 2 + s.wave }), Transform({ translation: [i, 0, -10] }), Alive());
  };
});

export const pause = system(() => {
  const next = nextState(Game);
  return () => {
    next.set(Game.Paused);
    // @ts-expect-error  not a Game value
    next.set("Paused");
  };
});

export const EnemySet = systemSet("game::Enemies");

// ---------- 3. A message and an observer ----------

export const applyDamage = system(() => {
  const damage = reader(Damage);
  const q = query(Mut(Health));
  const score = resMut(Score);
  const cmd = commands();
  return () => {
    damage.each((d) => {
      const [h] = q.get(d.target) ?? [];
      if (!h) return;
      h.hp -= d.amount;
      if (h.hp <= 0) {
        score.value += 1;
        cmd.trigger(Explode, d.target, { radius: 3 });
      }
    });
    // @ts-expect-error  amount is a number
    cmd.write(Damage, { target: undefined as never, amount: "lots" });
  };
});

export const onExplode = observer(On(Explode), () => {
  const q = query(EntityId, Transform).with(Enemy);
  const damage = writer(Damage);
  const cmd = commands();
  return (on) => {
    const [, at] = q.get(on.entity) ?? [];
    if (!at) return;
    q.each((e, t) => {
      if (t.translation.distance(at.translation) < on.event.radius) damage.write({ target: e, amount: 50 });
    });
    cmd.entity(on.entity).despawn();
  };
});

export const onHealthAdded = observer(Add(Health), () => {
  const q = query(Mut(Health));
  return (on) => {
    const [h] = q.get(on.entity) ?? [];
    if (h) h.hp = h.max;
  };
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
