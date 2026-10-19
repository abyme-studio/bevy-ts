# TS developer experience against Bevy's own, and adopting Rust Bevy plugins

1. **Surface:** adopt the Bevy-shaped API: `system({ params }, fn)`, `app.addSystems(Update, s.after(x).runIf(inState(...)))`, `plugin(app => ...)`, `observer(On(E), ...)`, resources, messages, states and `Local`. Systems are identified by `module#export`. Access comes from `Mut(...)` and is checked by `tsc`. `q.each` is the hot loop. There is no ParamSet, and there are no hooks in systems or in TSX: a binding reads `ref.get(C)`.
2. **Plugins:** take layouts from reflection, not from `repr(C)`. Any reflected component whose leaves are plain data becomes a zero-copy column with no generated code, Rapier's `Velocity` included. Adopting a plugin is one line in `bevy-ts.json`. bevy-ts publishes thin curated ports only for what reflection cannot reach: Rapier now, and leafwing, avian and hanabi once they are on Bevy 0.20.

Researched and measured 2026-10-19 on this Mac (Apple M5 Max, macOS 25.4; deno 2.9.7 with V8 15.0.245 and TypeScript 6.0.3; rustc 1.98). Bevy 0.20.0 source is from `~/dev/refs/bevy-0.20`. Plugin sources are from crates.io: `bevy_rapier3d` 0.37.0, `avian3d` 0.7.0, `bevy_hanabi` 0.19.0, `leafwing-input-manager` 0.22.0 and `bevy_tweening` 0.16.0.

The evidence is in the repo:

- `bench/dx/`: the proposed API as types (`bevy-ts/mod.ts`, `jsx-runtime.ts`, `rapier.ts`). It also holds the examples below as real `.ts`/`.tsx` files. `deno check examples/*.ts examples/*.tsx` passes. Every `@ts-expect-error` in them fires.
- `bench/dx/iter-forms.js`: iteration forms, with results in `bench/dx/results/`.
- `bench/ports/`: the `offset_of!` and reflection proof against `bevy_rapier3d` 0.37. `cargo test` runs 5 tests and 4 `compile_fail` cases, and all pass. Output is in `bench/ports/results/cargo-test.txt`.

## Side by side

Each case shows three versions: Rust Bevy 0.20, bevy-ts as the specs stand today (bridge, tsx, native), and bevy-ts as proposed. The proposed code is excerpted from `bench/dx/examples/`.

### A system with two queries and a resource

Rust Bevy:

```rust
fn chase(
    time: Res<Time>,
    player: Single<&Transform, With<Player>>,
    mut enemies: Query<(&Enemy, &mut Transform), (With<Alive>, Without<Player>)>,
) {
    for (enemy, mut t) in &mut enemies {
        let dir = (player.translation - t.translation).normalize();
        t.translation += dir * enemy.speed * time.delta_secs();
    }
}
```

bevy-ts today:

```ts
// One query per system. The player's position needs a second system and a component to carry it.
app.system("chase", { stage: "update", read: [Enemy, PlayerPos], write: [Transform], with: [Alive], run: (q, ctx) => {
  q.each((e, enemy, target, t) => {
    const dx = target.x - t.translation.x, dy = target.y - t.translation.y, dz = target.z - t.translation.z;
    const len = Math.hypot(dx, dy, dz) || 1;
    t.translation.x += (dx / len) * enemy.speed * ctx.dt; /* and y, z */
  });
}});
```

Proposed:

```ts
const toPlayer = vec3(); // scratch vector, made once

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
      enemy.speed = 3;          // tsc error: Enemy is read-only in this query
    });
  },
);
```

### Ordering, run conditions, states

Rust Bevy:

```rust
#[derive(States, Default, Clone, PartialEq, Eq, Hash, Debug)]
enum Game { #[default] Menu, Playing, Paused }

app.init_state::<Game>()
   .add_systems(OnEnter(Game::Playing), spawn_wave)
   .add_systems(Update, (
       (spawn_wave, chase, separate).chain().in_set(EnemySet).run_if(in_state(Game::Playing)),
       apply_damage.after(EnemySet),
   ));
fn pause(mut next: ResMut<NextState<Game>>) { next.set(Game::Paused); }
```

bevy-ts today:

```ts
// No states and no run conditions. Ordering is by name within a stage.
app.system("chase", { stage: "update", after: ["spawnWave"], ... });
// A "playing" check is a component or a field the system reads and returns early on.
```

Proposed:

```ts
export const Game = states("game::Game", ["Menu", "Playing", "Paused"]);
export const EnemySet = systemSet("game::Enemies");

app.initState(Game, Game.Menu)
  .addSystems(OnEnter(Game.Playing), spawnWave)
  .addSystems(Update,
    chain(spawnWave, chase, separate).inSet(EnemySet).runIf(inState(Game.Playing)),
    applyDamage.after(EnemySet));

export const pause = system({ next: NextState(Game) }, ({ next }) => next.set(Game.Paused));
// next.set("Paused") is a tsc error.
```

`inState`, `resourceExists` and `onMessage` are data, so Rust evaluates them and a skipped system costs no JS call.

### A message and an observer

Rust Bevy 0.20 has three kinds:

- `Message`: buffered, read with `MessageReader`.
- `Event`/`EntityEvent`: run observers through `On<E>`, triggered with `commands.trigger`.
- Lifecycle patterns: `On<Add<C>>`, `Insert`, `Discard`, `Remove` and `Despawn`.

```rust
#[derive(Message)] struct Damage { target: Entity, amount: f32 }
#[derive(EntityEvent)] struct Explode { entity: Entity, radius: f32 }

fn apply_damage(mut damage: MessageReader<Damage>, mut q: Query<&mut Health>, mut commands: Commands) {
    for d in damage.read() {
        let Ok(mut h) = q.get_mut(d.target) else { continue };
        h.hp -= d.amount;
        if h.hp <= 0.0 { commands.trigger(Explode { entity: d.target, radius: 3.0 }); }
    }
}
fn on_explode(ev: On<Explode>, q: Query<(Entity, &Transform), With<Enemy>>, mut out: MessageWriter<Damage>) { /* ... */ }
app.add_message::<Damage>().add_observer(on_explode);
```

bevy-ts today: none. TS-defined resources and events are a bridge non-goal. The only reactions are TSX `onClick` handlers.

Proposed:

```ts
export const Damage = message("game::Damage", { target: entity, amount: f32 });
export const Explode = entityEvent("game::Explode", { radius: f32 });

export const applyDamage = system(
  { damage: MessageReader(Damage), q: Query([Mut(Health)]), score: ResMut(Score), commands: Commands },
  ({ damage, q, score, commands }) => {
    damage.each((d) => {
      const [h] = q.get(d.target) ?? [];
      if (!h) return;
      h.hp -= d.amount;
      if (h.hp <= 0) { score.value += 1; commands.trigger(Explode, d.target, { radius: 3 }); }
    });
  });

export const onExplode = observer(On(Explode),
  { q: Query([EntityId, Transform], { with: [Enemy] }), damage: MessageWriter(Damage) },
  (on, { q, damage }) => {
    const [, at] = q.get(on.entity) ?? [];
    if (!at) return;
    q.each((e, t) => {
      if (t.translation.distance(at.translation) < on.event.radius) {
        const m = damage.next(); m.target = e; m.amount = 50; // allocation-free write
      }
    });
  });

export const onHealthAdded = observer(Add(Health), { q: Query([Mut(Health)]) }, (on, { q }) => { /* ... */ });
```

### A plugin

Rust Bevy:

```rust
pub struct EnemyPlugin;
impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>().add_message::<Damage>().init_state::<Game>()
           .add_systems(Update, (chase, apply_damage)).add_observer(on_explode);
    }
}
app.add_plugins(EnemyPlugin);
```

bevy-ts today: none. A module calls `app.system` at top level.

Proposed:

```ts
export const EnemyPlugin = plugin((app) => {
  app.initResource(Score, { value: 0 }).addMessage(Damage).initState(Game, Game.Menu)
    .addSystems(Update, chain(spawnWave, chase, separate).inSet(EnemySet).runIf(inState(Game.Playing)))
    .addObserver(onExplode);
});
app.addPlugins(EnemyPlugin);
```

A plugin is a function that the loader re-runs on every generation. Nothing about it outlives a reload except the ECS state it set up.

### Spawning a prefab

Rust Bevy 0.20 (BSN):

```rust
fn grunt(at: Vec3, speed: f32) -> impl Scene {
    bsn! {
        #Grunt
        Transform { translation: {at} } Enemy { speed: {speed} } Health { hp: 100.0, max: 100.0 } Alive
        on(|click: On<PointerClick>, mut commands: Commands| { commands.trigger(Explode { entity: click.entity, radius: 3.0 }) })
    }
}
commands.spawn_scene(grunt(vec3(0.0, 0.0, -12.0), 3.0));
```

bevy-ts today (tsx): a prefab function and `ctx.commands.mount(<Enemy at={p} />)`. A handler is `onClick` on `<button>` only.

Proposed:

```tsx
export const Grunt = ({ at, speed = 2 }: { at: readonly [number, number, number]; speed?: number }) => (
  <entity name="grunt" onPointerClick={(_, { entity, commands }) => commands.trigger(Explode, entity, { radius: 3 })}>
    <Transform translation={at} />
    <Enemy speed={speed} />
    <Health initialHp={100} max={100} />
    <Alive />
    <RigidBody value="Dynamic" />
    <Collider cuboid={[0.5, 0.5, 0.5]} />
    <mesh shape="cuboid" size={[1, 1, 1]} color="#c33" />
  </entity>
);

// in a system
commands.spawn(<Grunt at={[i * 2, 0, -12]} speed={3} />);
```

`<entity onPointerClick>` is an entity observer, which is what `on(...)` is in BSN. `commands.spawn` takes components (`Health({ hp: 3 })`) or a prefab element, so `mount` folds into `spawn`.

### A UI HUD, with and without hooks

Rust Bevy:

```rust
#[derive(Component)] struct HpText;
fn hud() -> impl Scene {
    bsn! { Node { position_type: PositionType::Absolute, top: px(12), left: px(12) } Children [ (HpText Text("")) ] }
}
fn update_hp(player: Single<&Health, (With<Player>, Changed<Health>)>, mut text: Single<&mut Text, With<HpText>>) {
    text.0 = format!("HP {} / {}", player.hp.ceil(), player.max);
}
```

bevy-ts today (tsx), with a hook:

```tsx
export const HudWithHooks = ({ player }: { player: NodeRef }) => {
  const health = useComponent(player, Health); // a hook: must run in the prefab body
  return (
    <node style={{ position: "absolute", top: px(12), left: px(12) }} background="#0008">
      <text size={20}>{() => `HP ${Math.ceil(health().hp)} / ${health().max}`}</text>
    </node>
  );
};
```

Proposed, with no hook:

```tsx
export const Hud = ({ player }: { player: NodeRef }) => (
  <node style={{ position: "absolute", top: px(12), left: px(12) }} background="#0008">
    <text size={20}>{() => `HP ${Math.ceil(player.get(Health).hp)} / ${player.get(Health).max}`}</text>
    <text size={16} color={() => (res(Score).value > 10 ? "#fd0" : "#fff")}>{() => `Score ${res(Score).value}`}</text>
    <button onPointerClick={(_, { commands }) => commands.trigger(Explode, player.entity, { radius: 5 })}>
      <text>Boom</text>
    </button>
  </node>
);
```

`player.get(Health)` inside `() => ...` is a tracked read. The site learns its reads on first evaluation, as a system learns its query (tsx decision 11). Rust re-runs the site only when that row's `Health` tick moves. This is decision 9's per-pair check, with no JS call in a quiet frame. Outside a binding, `player.get` is a dev error naming the location.

Both versions cost the same. The hook buys nothing the binding does not already know, and it adds a placement rule ("call it in the body, not in the binding"). It also hands agents the React idiom: `useComponent` is one step from `useState` and `useEffect`, which is how bevy-ts would become bevy-react.

A hook-like API is still the right tool in two places, and neither is named `use*`:

- `signal()`: UI-local state that is not game state, such as a dropdown's open flag or a text field's draft. Components run once, so a signal has no call-order rule.
- A declarative list over a query, for an inventory or a scoreboard: `<For each={Query([EntityId, Item], { with: [InBag] })}>{(e, item) => ...}</For>`. This one needs a subscription whose lifetime is the UI subtree. It is tsx's open `useQuery` question, answered with a query descriptor instead of a hook.

Systems never get hooks: their params are their subscriptions.

### Adopting Rapier, end to end

Rust Bevy:

```toml
# Cargo.toml
bevy_rapier3d = "0.37"
```

```rust
app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default());
commands.spawn((RigidBody::Dynamic, Collider::cuboid(0.5, 0.5, 0.5), Velocity::zero(), Transform::default()));
fn steer(mut q: Query<(&Enemy, &Transform, &mut Velocity)>, player: Single<&Transform, With<Player>>) {
    for (enemy, t, mut v) in &mut q { v.linear = (player.translation - t.translation).normalize() * enemy.speed; }
}
fn hurt(mut contacts: MessageReader<CollisionEvent>, mut q: Query<&mut Health, With<Player>>) {
    for c in contacts.read() { if let CollisionEvent::Started(a, b, _) = c { /* ... */ } }
}
fn grounded(rapier: ReadRapierContext, q: Single<(Entity, &Transform), With<Player>>) {
    let ctx = rapier.single().unwrap();
    let hit = ctx.cast_ray(q.1.translation, Vec3::NEG_Y, 1.1, true, QueryFilter::default().exclude_rigid_body(q.0));
}
app.add_systems(FixedUpdate, steer.before(PhysicsSet::SyncBackend));
```

bevy-ts today (native): a native crate, and Rust glue for every type TS should write.

```rust
// native/src/lib.rs, hand-written
#[derive(Component, Reflect, NativeLayout, Default)] #[repr(C)]
struct TsVelocity { linear: Vec3, angular: Vec3 }           // mirror: Velocity is not repr(C)
fn sync_in(mut q: Query<(&TsVelocity, &mut Velocity)>) { for (a, mut b) in &mut q { b.linear = a.linear; b.angular = a.angular; } }
fn sync_out(/* the reverse, after the step */) {}
#[derive(Message, Reflect, NativeLayout)] #[repr(C)] struct TsContact { a: Entity, b: Entity, started: u8 }
fn forward_contacts(/* CollisionEvent -> TsContact */) {}
fn cast_ray(/* native function, batched records in and out */) {}
bevy_ts::native!(|app| { app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default()); /* expose sets, register all of the above */ });
```

```ts
app.system("steer", { stage: "fixedUpdate", read: [Enemy, Transform], write: [TsVelocity], after: ["physics:before"], run });
```

Proposed:

```jsonc
// bevy-ts.json
{ "native": { "ports": ["rapier3d"] } }
```

```tsx
import { CollisionEvent, PhysicsSet, RapierContext, Velocity, RigidBody, Collider } from "bevy-ts/rapier";

// TSX: <RigidBody value="Dynamic" /> <Collider cuboid={[0.5, 0.5, 0.5]} />, as in Grunt above.

export const steer = system(
  { player: Query([Transform], { with: [Player] }), enemies: Query([Enemy, Transform, Mut(Velocity)]) },
  ({ player, enemies }) => {
    const [p] = player.single();
    enemies.each((enemy, t, v) => {          // v is Rapier's own Velocity column, zero-copy
      dir.subVectors(p.translation, t.translation).normalize();
      v.linear.x = dir.x * enemy.speed;
      v.linear.z = dir.z * enemy.speed;
    });
  });

export const hurt = system(
  { contacts: MessageReader(CollisionEvent), q: Query([Mut(Health)], { with: [Player] }) },
  ({ contacts, q }) => contacts.each((c) => {
    if (c.kind !== "Started") return;
    const [h] = q.get(c.a) ?? q.get(c.b) ?? [];
    if (h) h.hp -= 10;
  }));

export const grounded = system(
  { rapier: RapierContext, q: Query([EntityId, Transform, Mut(Velocity)], { with: [Player] }) },
  ({ rapier, q }) => {
    const [e, t, v] = q.single();
    if (!rapier.castRay(t.translation, down, 1.1, true, { exclude: e }) && v.linear.y > -20) v.linear.y -= 0.5;
  });

app.addSystems(FixedUpdate, steer.before(PhysicsSet.SyncBackend), grounded.before(PhysicsSet.SyncBackend))
   .addSystems(Update, hurt);
```

The project writes no Rust. `bevy-ts` generates the native crate from `bevy-ts.json` and builds the project engine once per Rust change, or fetches it from the shared cache (native decisions 1 and 2).

## Evidence, question 1: the TS surface

### What `tsc` proves (`bench/dx/`, `deno check`, TypeScript 6.0.3)

- **Query tuples infer exactly.** `Query([EntityId, Enemy, Mut(Transform), Option(Health)])` gives `[Entity, Ref<Enemy>, MutRef<Transform>, Ref<Health> | undefined]`. `examples/inference.ts` asserts each element with an exact type-equality check, and flipping one assertion fails the check. The mechanism is a `const` type parameter on `Query` plus a mapped tuple type. No transformer is involved, unlike Thyseus.
- **Read and write are enforced at compile time.** Fields of a read component are `readonly`. A read vector is `ReadonlyVec3`, which has no mutating methods. A `Mut` vector field is a view: assignable by component (`t.translation.y += 1`) and by method (`t.translation.addScaled(v, s)`), but not reassignable (`t.translation = v` is an error). Misspelt fields are errors. The bridge spec today only enforces this at load.
- **Several queries per system** come from a params object resolved by a mapped type: `system({ time: Res(Time), a: Query(...), b: Query(...) }, ({ time, a, b }) => ...)`.
- **Filters** are `{ with, without, changed, added }`. `changed` reads bridge's per-row ticks against the system's last run (bridge decision 6). An `Or` filter is rare enough to defer.
- **Tiers show in the types.** `Mut(Collider)` is a type error (`Collider` is reflect tier), and `{ with: [Collider] }` is fine. States, messages and TSX props are typed too: `next.set("Paused")`, `<RigidBody value="Floating" />`, `<Transform translation={[1, 2]} />` and `onPointerClick` on `<mesh>` are all errors (`examples/tsx-errors.tsx`).
- **Not enforced by types:** giving both `hp` and `initialHp` (the load error stays), and a system that is not exported (a load error, below).

### ParamSet is not needed

Bevy needs `ParamSet` because two queries that may alias `&mut T` break Rust's borrow rules and stop the executor from running them in parallel. Neither applies to bevy-ts:

- The stage is exclusive and JS is single-threaded (bridge decision 1).
- Two queries over one column are two views of the same memory, so a write through one is visible through the other.

`examples/systems.ts` `separate` reads and writes `Transform` through two overlapping queries. What does change is bookkeeping:

- A column is a `write` column if any query in the system writes it.
- The dev-only read-column guard hashes only columns no query writes.

### Iteration: `q.each` is the hot loop; `for...of` is sugar

V8 does not remove the iterator protocol from a custom iterable. The table is the median of 60 runs over one table with three forms:

- `[Enemy, Mut(Transform)]`, `t.translation.y += enemy.speed * dt` through tick-storing accessors.
- `addScaled`, which writes three fields.
- Raw arrays (`bench/dx/results/iter-forms.md`).

| form | 1M rows, V8 JIT | 10k rows, V8 JIT | 100k rows, jitless |
|---|---|---|---|
| `q.each((enemy, t) => ...)` | 2.18 ms | 0.020 ms | 11.7 ms |
| `for (const [enemy, t] of q)` | 5.34 ms | 0.051 ms | 21.2 ms |
| `q.each`, `t.translation.addScaled(v, s)` | 6.04 ms | 0.060 ms | 14.1 ms |
| raw arrays (`q.tables`) | 0.63 ms | 0.017 ms | 3.2 ms |

- `for...of` costs 2.4× `each` with the JIT and 1.8× without it. It reads like Bevy and is fine for the player, a camera or a few hundred rows. The docs and the generated examples should use `each` for anything that scales, and `single()` and `get(e)` for one row.
- `addScaled` writes all three components and stamps the tick three times. The setters should stamp once per row (a "dirty this row" flag, which `change-detection.md` measured), or vector methods should write the tick once.
- Nested vector views cost about 3.5× raw arrays at 1M rows in this harness, against roughly 1× for flat accessors in `change-detection.md`. The row is propagated through a second object on every row. The accessor generator should hand a vector view the parent's row by reference, not by a setter. This is worth a bridge task (11.2) before the API settles.

### Vector maths without operator overloading

The API is glam-like and in place: every method writes `this` and returns it. Scratch vectors are made once with `vec3()`, at module scope or in a `Local`. Column views and scratch vectors share one interface, so `toPlayer.subVectors(a.translation, b.translation).normalize()` allocates nothing.

This is three.js's `Vector3` and gl-matrix's out-param style with glam's names. becsy, koota and bitECS ship no maths API; Thyseus had methods on component classes. Operators are out of reach: no JS engine overloads them.

### Bevy 0.20 names, checked in source

| Bevy 0.20 | Where | Proposed TS |
|---|---|---|
| `Message`, `MessageReader`, `MessageWriter`, `add_message` | `bevy_ecs/src/message/` | `message()`, `MessageReader()`, `MessageWriter()`, `addMessage` |
| `Event`, `EntityEvent`, `On<E>`, `commands.trigger` | `event/mod.rs`, `observer/system_param.rs` | `event()`, `entityEvent()`, `On(E)`, `commands.trigger` |
| `On<Add<C>>`, `Insert`, `Discard`, `Remove`, `Despawn` | `lifecycle.rs` (`EventPattern` for each) | `Add(C)`, `Insert(C)`, `Remove(C)` |
| `On<PointerClick>` (not `Pointer<Click>`) | `bevy_picking/src/events.rs`, `examples/ui/navigation` | `onPointerClick`, `On(PointerClick)` |
| `in_state`, `on_message`, `run_if` | `bevy_state/src/condition.rs`, `schedule/condition.rs` | `inState`, `onMessage`, `runIf` |
| `Local<T>`, `ParamSet` | `system/system_param.rs` | `Local(schema)`; no ParamSet |
| `trait Resource: Component`; `ReflectResource` implies `ReflectComponent` | `resource.rs`, `reflect/resource.rs` | `resource()` is a component on a resource entity |

The last row matters most. In 0.20 a resource is a component on its own entity, so `Res(Score)` is a one-row column through the same path as a query. TS resources need no separate storage, and native's open question about "native resources as single-row views" answers itself.

### Hot-reload identity by `module#export`

- The loader walks each module's namespace and stamps every exported system, observer and plugin with `"src/enemy.ts#chase"`. That is the key for ordering, per-system `changed` ticks, message cursors and `Local` storage.
- `Local({ timer: f32 })` is a layout row kept in Rust under that key, so it survives reloads, as the hot-reload rules require of anything that must live.
- A system that is not exported fails the load, with its location.
- Renaming an export is a new system, and its `Local` starts at defaults. It is reported as such.

### Observers across the boundary

A Bevy observer runs at once when its trigger applies.

- **Triggered from a TS system:** the trigger is a command, applied right after the system returns (bridge decision 7). The observer can run there, in the same stage, as one more JS call.
- **Triggered from Rust** (lifecycle on a Rust insert, `PointerClick`): calling JS from inside an arbitrary Rust command flush would make the runtime re-entrant from any exclusive context. These triggers are queued with their propagation path, computed in Rust, and run at the next bevy-ts stage boundary in the same frame, in trigger order, in one call.
- **This is the one documented difference from Bevy.** It is the same latency tsx decision 9 already accepts for picking.

### What other TS ECS libraries do

Read 2026-10-19:

- **koota:** best tuple inference (`updateEach(([pos, vel]) => ...)`). Writing is by method, not declared per system. It has no scheduler and no plugins.
- **becsy:** best declared access (`q.current.with(A).write`, `schedule(s => s.before(B))`). Its rows are untyped `Entity` handles.
- **Thyseus 0.18:** the closest to Bevy (`Query<[A, B]>`, `Res`, `Commands`, `EventReader`), but its params come from a compile-time TypeScript transformer. It has no `Mut`, and it has been unmaintained since May 2024.
- **bitECS 0.4 and miniplex 2:** no access model and no inference beyond the entity type.

None of them combines inferred tuples with declared mutability. The `const` type parameter does it with no transformer, which matters because bevy-ts compiles with oxc and has no plugin pipeline (native decision 9).

### What the Bevy scripting crates do

- **bevy_mod_scripting 0.23:** a builder: `system_builder("name", script):query(world.query():component(A):with(C)):resource(R):after(other)`. Access is declared, nothing is typed, and access is through reflection per field.
- **bevy_mod_js_scripting:** abandoned. It used `world.query(Transform, Aabb)` with type names as strings.
- **bevy-react:** has no query API, only JSON messages.

The proposed surface is bevy_mod_scripting's declared-access model with the types Lua cannot have, over columns instead of reflection.

## Evidence, question 2: adopting a Rust plugin

### `offset_of!` on a third-party, non-`repr(C)` struct (`bench/ports/`, rustc 1.98)

| Case | Result |
|---|---|
| `offset_of!(bevy_rapier3d::prelude::Velocity, linear)`, `angular` (public fields, no `repr`) | compiles: size 24, align 4, `linear` @0, `angular` @12 |
| nested: `offset_of!(Velocity, linear.y)`, `offset_of!(Transform, scale.x)` | compiles (nested paths stable since 1.82) |
| tuple struct: `offset_of!(GravityScale, 0)` | compiles |
| `Transform` | size 48, align 16: rustc puts `rotation` first (@0), then `translation` @16 and `scale` @28 |
| private field: `offset_of!(LockedAxes, 0)` | E0616 "field `0` of struct `LockedAxes` is private" |
| enum variant: `offset_of!(CollisionEvent, Started.0)` | E0658 "using enums in offset_of is experimental" (#120141) |
| glam SIMD: `offset_of!(Quat, x)`, `offset_of!(Vec3A, y)` | E0609 "no field": on neon and sse2 they wrap a vector register and reach `x` through `Deref`. Scalar targets (wasm without simd128) have real fields, so this one is per target |
| zero-copy write | 1,000 Rapier `Velocity` rows written through an `f32` view at `row * 24 + offset`, plus a changed tick, then read back through Bevy's own `Query` |

`offset_of!` needs no `repr(C)`. A project engine is compiled anyway, so offsets from that compile are the truth for that binary. JS already only reads offsets Rust hands it (bridge decision 3).

### Reflection gives the same offsets with no generated code

- **The method:** a derived `Reflect` hands out references into the value itself, so `field_at(i)` minus the value's address is the field's offset.
- **It agrees with `offset_of!`** on every field of `Velocity` and `Transform`.
- **It reaches what `offset_of!` cannot:**
  - `rotation.x/y/z/w` inside glam's SIMD `Quat` (@0, 4, 8, 12);
  - the private `LockedAxes.0` (`u8` @0).
- **It needs a value:**
  - `ReflectDefault`, which Rapier's components register;
  - `ReflectFromWorld`;
  - or the first live row.
- **The engine can do this at startup for every registered component.** So the reflect tier and the layout tier stop being a choice the author makes. A component is layout tier when every reflected leaf is a plain-data type bevy-ts knows (`f32`, `f64`, the integers, `bool`, `Entity`, and glam's vectors and quaternions). This holds whatever its `repr`.
- **Transform stops being special.** Bridge decision 4's hand-registered `Transform` becomes the first case of the general rule.

Guards on that rule:

- **Bounds:** every derived offset lies inside `size_of::<T>()`, leaves do not overlap, and leaf sizes match their `TypeId`. Otherwise the component is reflect tier, with the reason in `bevy-ts types`. This rejects a custom `Reflect` impl that hands out references elsewhere.
- **Validity:**
  - `bool`, fieldless enums and `Entity` are accessor-only. Their setters write `0`/`1`, a byte from the variant table, or a validated id.
  - `q.tables` does not expose them raw, because a stray byte there is undefined behaviour in Rust, unlike a stray `f32`.
  - The variant table is read from this build at startup (`RigidBody`: `[0, 1, 2, 3]`), never assumed.
- **Mutability:** a component declared `#[component(immutable)]` is never writable. Field writes run no hooks, as with Bevy's `Mut`.
- **Privacy:** reflected private fields become writable, as they already are to `bevy_remote`'s `world.mutate_components`. `#[reflect(ignore)]` fields are never touched.

### What reflection does not reach, from the five plugins

| Plugin, version, Bevy | What is data (generated) | What needs a curated port |
|---|---|---|
| `bevy_rapier3d` 0.37, Bevy 0.20 | `Velocity`, `ExternalImpulse`, `ExternalForce`, `GravityScale`, `Damping`, `Ccd`, `Sleeping`, `RigidBody` (enum table), `LockedAxes`, `ActiveEvents` (private bitflags, via reflection) | `Collider` (`Arc<dyn Shape>`, `// TODO: Reflect`) as TSX constructors; `CollisionEvent` (a `Message` enum with data, no `Reflect`) flattened to `{ kind, a, b, sensor }`; `cast_ray` and friends as native functions; `PhysicsSet` names; `RapierPhysicsPlugin::<NoUserData>` construction |
| `avian3d` 0.7, Bevy 0.19 | `LinearVelocity(pub Vector)` and the rest; `Vector` is `DVec3` under `f64`, which bridge's `f64` covers | spatial queries, collider constructors; waits for Bevy 0.20 |
| `bevy_hanabi` 0.19, Bevy 0.19 | `EffectSpawner` | `ParticleEffect` holds a `Handle`, `EffectProperties` is a list of named values: a set-property-by-name function; waits for 0.20 |
| `leafwing-input-manager` 0.22, Bevy 0.20 | nothing useful | `ActionState<A>` is generic over the game's own Rust enum. A port ships `TsAction(u16)` implementing `Actionlike`, with names and kinds from a TS declaration, so new actions need no Rust rebuild |
| `bevy_tweening` 0.16, Bevy 0.19 | nothing: `TweenAnim` holds `Box<dyn Tweenable>` | none. Tweening belongs in TS over columns, where it reloads |

Generics such as `RapierPhysicsPlugin<NoUserData>` only matter where the plugin is constructed, which is one generated line. Components are registered as concrete types, so the registry walk sees `Velocity`, never `T`. `Handle<T>` and `Option<Entity>` are not plain data: `Option<Entity>` is 8 bytes, with `None` in a niche rustc picks. They stay reflect tier, with asset paths and entity-or-null in the protocol's JSON.

Events, resources and sets:

- **Resources** are components in 0.20, so they take the same path.
- **Messages:** `ReflectMessage` can write a message but has no read path, so reading a plugin's `Messages<M>` needs monomorphised Rust per type. That is a generated line (`bevy_ts::expose_message::<CollisionEvent>()`) when `M: Reflect`, and a curated flattening when it is not, as Rapier's is not.
- **System sets** are not reflected at all, so exposing one is a generated line naming it, or part of a curated port.

### How a project declares adoption

One line in `bevy-ts.json`, never code:

```jsonc
{
  "native": {
    "ports": ["rapier3d"],                                   // curated: crate, version, plugin, sets, functions
    "crates": { "bevy_kira_audio": { "version": "0.27", "plugin": "bevy_kira_audio::AudioPlugin" } }  // uncurated
  }
}
```

- **What bevy-ts generates:** `.bevy-ts/engine/`, as native decision 1 already does. It adds the dependencies, the plugin lines, `expose_message`/`expose_set` lines for what the config names, and nothing per component.
- **Types:** they come from the registry walk (native decision 6), with layout or reflect tier decided per component by the rule above.
- **The native crate stays for the project's own Rust.** A project that only adopts plugins has none.
- **Beaten:**
  - *Cargo metadata* (`[package.metadata.bevy-ts]`): it needs a `Cargo.toml` the project would not otherwise have.
  - *Code* (`bevy_ts::native!`): it brings back the Rust the line removes.

### Curated or generate all

- **Generate the data layer for everything:** every reflected component, and every resource, is typed and either layout tier or reflect tier. This costs no code per type, so there is nothing to curate.
- **Curate the behaviour layer for a short list:**
  - `rapier3d` now (the only physics engine on Bevy 0.20, and native's ruling names it);
  - `leafwing-input-manager` (needs a dynamic `Actionlike`);
  - `avian3d` and `bevy_hanabi` when they reach 0.20.
- **What a port contains:** a Rust crate `bevy-ts-<name>` at bevy-ts's version, plus a `.d.ts` like `bench/dx/bevy-ts/rapier.ts`. It holds constructors, flattened messages, native functions, set names and the plugin line, a few hundred lines each.
- **Why not all curated:** curated-only would make bevy-ts the bottleneck for every crate.
- **Why not all generated:** it cannot produce a raycast API or a collider constructor from types.

## What changes in the specs (not edited; the operator rules first)

### bridge

- `ts-systems`:
  - **Replace** `app.system(name, { stage, read, write, with, without, after, run })` with `system(params, run)` and `app.addSystems(schedule, ...)`.
  - **Params:** `Query(data, filter)`, `Res`, `ResMut`, `Local`, `Commands`, `MessageReader`, `MessageWriter`, `NextState`, `State`.
  - **Access:** derived from query data (`Mut(C)` writes) as the union over a system's params. Several queries per system, overlapping allowed, no ParamSet.
  - **Filters:** `with`, `without`, `changed`, `added`. Query data adds `EntityId` and `Option`.
  - **Iteration:** `q.each` as the hot loop, plus `for...of`, `single`, `get`, `len` and `tables`.
  - **Schedules:** `Update`/`FixedUpdate`/... map onto the existing stages. Ordering is `.after`/`.before`/`.inSet`/`chain` by object, not by name string.
  - **Run conditions:** `inState`, `resourceExists` and `onMessage` are evaluated in Rust, so a skipped system makes no JS call.
- `ts-components`:
  - **Typed views:** read views are typed `readonly`, write views are `MutRef`, and vector fields are in-place views with the glam-like method set.
  - **Scratch vectors:** `vec3()`/`quat()` make scratch values.
  - **Dual handles:** a component handle is also its TSX tag and its bundle constructor.
- `hot-reload`:
  - **Identity:** systems, observers and plugins are identified by `module#export`.
  - **Exports:** an unexported system is a load error.
  - **`Local`:** rows are kept in Rust under that identity and survive reloads.
  - **Renames:** renaming an export is reported as a new system.
- **Non-goals:** "TS-defined resources and events" moves into scope.
  - *Resources:* components on resource entities (0.20).
  - *Messages:* double-buffered arenas per type, with a cursor per system.
  - *Events and observers:* `observer(On(E), params, run)` and lifecycle `Add`/`Insert`/`Remove`. A TS-triggered observer runs when its command applies; a Rust-triggered one runs at the next stage boundary.
  - *States:* TS states, with `OnEnter`/`OnExit` run by one bevy-ts exclusive system in `StateTransition`.
  - *Plugins:* `plugin(build)`, re-run per generation.
- Decision 4 and `ts-components` "Rust components cross only with a declared native layout": layouts come from reflection over a value, bounds-checked, for any registered component whose reflected leaves are plain data. `offset_of!` stays for `NativeLayout` derives and for unreflected types in ports. `Transform` stops being a special case.
- Decision 5 and task 11.2: vector views get the parent's row without a per-row setter. Setters stamp a row's tick once per row, not once per field.
- `commands.spawn` accepts prefab elements; tsx B3's `commands.mount` folds into it.

### tsx

- **`useComponent`:** removed in favour of `ref.get(C)` and `res(R)` read inside bindings, resolved per site (decision 11) or per pair (decision 9). No `use*` names. `signal()` stays for UI-local state.
- **`useQuery`:** the open question becomes `<For each={Query(...)}>`.
- **Picking:** handlers are entity-local observer props (`onPointerClick`, `onPointerOver`, ...) on any `<entity>` or UI element, named after Bevy 0.20's `PointerClick`. The design's `Pointer<Click>` is the 0.19 name.
- **B3:** becomes `commands.spawn(<Prefab />)`.

### native

- **Decision 5:** the two tiers are redrawn. Layout tier no longer requires `#[repr(C)]` or a derive: any reflected component whose leaves are plain data qualifies, with the guards above. `#[derive(NativeLayout)]` stays for unreflected types. The "What stays Rust" row for third-party plugins changes from "their components with `NativeLayout` if the crate author or a wrapper adds it" to "their reflected components, as columns when their leaves are plain data".
- **New decision:** adoption by `bevy-ts.json` (`native.ports`, `native.crates`). Generated plugin, message and set lines go in `.bevy-ts/engine/`. Curated ports are published as `bevy-ts-<name>` crates with `bevy-ts/<name>` types. A native crate is needed only for the project's own Rust.
- **Open question closed:** native resources are components in Bevy 0.20, so they take the layout path.
- **Decision 8:** a native function may also be exposed as a system param (`RapierContext`) with single and batched calls. A single call is one host call, fine for a handful of rays a frame; batches stay the rule for many.
- **N1:** the layout registry is filled from the type registry at startup, not only by derives.

## Sources

- Bevy 0.20.0, `~/dev/refs/bevy-0.20`:
  - `bevy_ecs/src/observer/system_param.rs` (`On`), `event/mod.rs` (`Event`, `EntityEvent`, `EventPattern`), `lifecycle.rs` (`Add`, `Insert`, `Discard`, `Remove`, `Despawn`), `message/` (`MessageReader`, `MessageWriter`);
  - `resource.rs` (`trait Resource: Component`), `reflect/resource.rs`, `reflect/message.rs` (write only), `system/system_param.rs` (`ParamSet`, `Local`), `entity/mod.rs` (`Entity`, `repr(C, align(8))`), `storage/table/mod.rs`;
  - `bevy_state/src/condition.rs`, `bevy_picking/src/events.rs`, `bevy_scene/src/spawn.rs`, `examples/ui/layout/ghost_nodes.rs`, `examples/ecs/observers.rs`.
- `bevy_reflect` 0.20 `impls/glam.rs` (`Vec3`, `Vec3A`, `Quat` reflected as structs); glam 0.33.12 `f32/neon/quat.rs` (`Quat(float32x4_t)`), `f32/vec3.rs` (`#[repr(C)]`).
- crates.io sources:
  - `bevy_rapier3d` 0.37.0: `dynamics/rigid_body.rs`, `geometry/collider.rs`, `pipeline/events.rs`, `plugin/plugin.rs`, `plugin/context/mod.rs`;
  - `avian3d` 0.7.0: `dynamics/rigid_body/mod.rs`, `math/double.rs`;
  - `bevy_hanabi` 0.19.0: `lib.rs`, `properties.rs`, `spawn.rs`;
  - `leafwing-input-manager` 0.22.0: `action_state/mod.rs`;
  - `bevy_tweening` 0.16.0: `lib.rs`.
- Rust: `core::mem::offset_of!` (stable 1.77, nested fields 1.82), `offset_of_enum` tracking issue https://github.com/rust-lang/rust/issues/120141.
- TS ECS libraries, read 2026-10-19: bitECS 0.4.0 https://github.com/NateTheGreatt/bitECS ; becsy 0.16.1 https://github.com/lastolivegames/becsy ; koota https://github.com/pmndrs/koota ; miniplex 2.0.0 https://github.com/hmans/miniplex ; Thyseus 0.18.0 https://github.com/JaimeGensler/thyseus (`packages/typescript-transformer`).
- Scripting crates: bevy_mod_scripting 0.23 `docs/src/ScriptSystems/introduction.md` (`~/dev/refs/bevy_mod_scripting`) ; bevy_mod_js_scripting https://github.com/jakobhellermann/bevy_mod_js_scripting ; bevy-react https://docs.rs/bevy-react.
- bevy-ts: `openspec/changes/{bridge,tsx,native,levels,viewport}`, `docs/research/change-detection.md`, `docs/research/scene-bindings.md`.
