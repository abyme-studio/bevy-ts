# TS developer experience against Bevy's own, and adopting Rust Bevy plugins

1. **Surface (ruled 2026-10-20):** systems are plain exported functions whose parameter types say what they use, as in Rust: `export function chase(time: Res<Time>, enemies: Query<[Enemy, Mut<Transform>], With<Alive>>) { ... }`. A syntax-only pass in bevy-ts's oxc compile step reads the annotations. A system whose param types are computed uses the setup closure instead. Around that:
   - Components, resources, messages and events are plain classes whose fields the same compile pass reads (ruled 2026-10-20): `class Health extends Component { hp: f32 = 100 }`. The name defaults to `module#Class`, and `static path = "game::Health"` pins it.
   - Systems register Bevy's way: `app.addSystems(Update, chain(a, b).runIf(inState(Game.Playing)))`. Observers are functions whose first parameter is `On<E>`; plugins are functions of `app`.
   - Systems are identified by `module#export`. Access comes from `Mut<...>` and is checked by `tsc`. `q.each` is the hot loop. There is no ParamSet.
   - No hooks in TSX (ruled 2026-10-20): a binding reads `ref.get(C)` and `res(R)`; `signal()` stays for UI-local state.
2. **Plugins (ruled 2026-10-20):** take layouts from reflection, not from `repr(C)`. Any reflected component whose leaves are plain data becomes a zero-copy column with no generated code, Rapier's `Velocity` included. Adopting a plugin is one line in `bevy-ts.json`. bevy-ts publishes thin curated ports only for what reflection cannot reach: Rapier now, and leafwing, avian and hanabi once they are on Bevy 0.20.

Researched and measured 2026-10-19 and 2026-10-20 on this Mac (Apple M5 Max, macOS 25.4; deno 2.9.7 with V8 15.0.245 and TypeScript 6.0.3; rustc 1.98). Bevy 0.20.0 source is from `~/dev/refs/bevy-0.20`. Plugin sources are from crates.io: `bevy_rapier3d` 0.37.0, `avian3d` 0.7.0, `bevy_hanabi` 0.19.0, `leafwing-input-manager` 0.22.0 and `bevy_tweening` 0.16.0.

The evidence is in the repo:

- `bench/dx/rust/`: every Rust snippet below, in one crate. `cargo check` passes against `bevy` 0.20 and `bevy_rapier3d` 0.37, so the Rust side is real Bevy, not remembered Bevy.
- `bench/dx/`: the ruled API as types (`bevy-ts/mod.ts`, `jsx-runtime.ts`, `rapier.ts`) and every TS snippet below as real `.ts`/`.tsx` files. `deno check examples/*.ts examples/*.tsx` passes. Every `@ts-expect-error` in them fires.
- `bench/dx/read-annotations.ts`: the syntax-only reader. `deno run -A bench/dx/read-annotations.ts bench/dx/examples/systems.ts` prints what the engine would declare for each system.
- `bench/dx/iter-forms.js`: iteration forms, with results in `bench/dx/results/`.
- `bench/ports/`: the `offset_of!` and reflection proof against `bevy_rapier3d` 0.37. `cargo test` runs 5 tests and 4 `compile_fail` cases, and all pass. Output is in `bench/ports/results/cargo-test.txt`.

## How systems declare what they use (ruled 2026-10-20)

A Rust system declares its parameters by type, and Bevy reads the types. TypeScript erases types before the code runs, so something has to turn the annotations into data. The ruling: bevy-ts's oxc compile step reads them from syntax alone, and a system whose types it cannot read uses the setup closure.

### What the reader does

It runs once per save on the watcher thread, beside oxc's type stripping, never per frame. For each exported function it reads the parameter annotations and attaches the declarations the engine needs:

```text
chase:        [time = res(Time), player = query(Transform).with(Player),
               enemies = query(Enemy, Mut(Transform)).with(Alive).without(Player)]
regen:        [q = query(Entity, Mut(Health), Option(Enemy)).changed(Health)]
spawnWave:    [commands = commands(), wave = local({ n: u32 })]
onExplode:    [explode = on(Explode), q = query(Entity, Transform).with(Enemy), damage = messageWriter(Damage), ...]
onHealthAdded:[add = on(Add(Health)), q = query(Mut(Health))]
grounded:     [rapier = rapier.readContext(), player = query(Entity, Transform, Mut(Velocity)).with(Player)]
```

That is the reader's real output on `bench/dx/examples/`. What it follows by name, without a type checker:

- inline param types;
- type aliases, including generic aliases (`type Movers<T> = Query<[T, Mut<Transform>]>`);
- param bundles, the way Rust's `#[derive(SystemParam)]` groups params: `interface Scoring { score: ResMut<Score>; time: Res<Time> }`;
- imports across files;
- param kinds that installed ports declare (`ReadRapierContext`).

It refuses param types TypeScript has to compute (`ReturnType<...>`, `Pick<...>`, conditional types), with the file and line: `fallback.ts:10: ReturnType is not a system param type, an alias of one, or a param bundle`. Rust has no equivalent of those: its system params are always written-out types.

### The fallback: a setup closure

```ts
export const drift = system(() => {
  const q = query(Enemy, Mut(Transform)).with(Alive);   // runs once at load: declares
  return () => q.each((enemy, t) => { t.translation.y += enemy.speed * 0.001; }); // every frame
});
```

The declaring calls (`query`, `res`, `resMut`, `commands`) may only run in the outer function; a call in the per-frame body is a load error.

### Outside the engine

The reader only attaches declarations and leaves the body alone, so source maps and breakpoints are unaffected. Any other tool that strips types (Node, Deno, vitest, esbuild) would load a system with no declarations, so:

- bevy-ts ships the same pass as a plugin for vitest, Vite and esbuild;
- `addSystems` throws "compiled without bevy-ts" for a function without declarations. It never runs silently with none.

### What was weighed, and why this one

The forms considered, for the same system (all but the builder and class were type-checked):

- **Params object:** `system({ time: Res(Time), enemies: Query(...) }, ({ time, enemies }) => ...)`. Every name is written twice.
- **Positional:** `system(Res(Time), Query(...), (time, enemies) => ...)`. Matched by order, so swapping two of the same kind is a silent bug.
- **Builder, zod/tRPC-style:** `system().res(Time).query(Enemy).run((time, enemies) => ...)`. Also matched by order.
- **Setup closure:** names once, no order bugs, but a `return () =>` wrapper on every system. Kept as the fallback.
- **Default params:** `system((time = res(Time), q = query(...)) => ...)`. Close to Rust, but the engine must call the body once at load to read the defaults. It runs until its first param use, which Rust never does.
- **Class, becsy-style:** `class Chase extends System { q = this.query(...); run() { ... } }`. Needs `this.` everywhere.
- **Type annotations (chosen):** the same shape as Rust.

State of the art, checked against primary sources on 2026-10-20:

- **No checker in the save loop.**
  - Nobody who derives runtime behaviour from TS types keeps a type checker in a fast build loop.
  - Thyseus, the one TS ECS that read system params from types, did it from syntax: it matched `Query` and `Res` by name.
  - Deepkit avoids the checker on purpose ("instantiating a TypeChecker per file/transformer is incredible slow").
  - oxc's own source says it has no type checker.
- **TypeScript 7 offers no usable checker API yet.**
  - 7.0 (the native Go compiler, 2026-07-08) shipped without an API. The new one is due in 7.1, which is still in dev builds.
  - It runs as a separate server process, and in one test it was 34–44% slower than TS 6 on cold builds.
  - Custom transformers aren't supported yet.
  - The only checker routes today are TS 6, a dead line, or linking into the Go internals, as typia and oxc's type-aware linter do.
- **No usable checker written in Rust:** stc is archived, ezno has stalled, and Biome only infers types for some lint rules.
- **Others have moved toward values:**
  - Angular moved dependency injection from constructor types to `inject()`; TS 5's standard decorators cannot emit parameter types.
  - NestJS 12 added an explicit value-based `schema` option.
  - becsy, the most complete live TS ECS, declares access with values.

  That is why the setup closure stays as the fallback.

A real type checker may come later as an optional lint, in CI or the editor, once TypeScript 7.1's API ships. It would never sit in the save loop.

What stays longer than Rust is vector maths: `dir.subVectors(a, b)` where Rust writes `a - b`. No JS engine overloads operators.

## Side by side

Rust Bevy 0.20 (from `bench/dx/rust/src/main.rs`, compiled) against the ruled TS (from `bench/dx/examples/`, type-checked). Under each pair is one line on what the specs allowed before the ruling.

### Declaring data

```rust
#[derive(Component)]
struct Health { hp: f32, max: f32 }

#[derive(Message)]
struct Damage { target: Entity, amount: f32 }

#[derive(EntityEvent)]
struct Explode { entity: Entity, radius: f32 }
```

```ts
export class Health extends Component { hp: f32 = 100; max: f32 = 100 }
export class Damage extends Message { declare target: Entity; declare amount: f32 }
export class Explode extends EntityEvent { declare radius: f32 }
```

Ruled 2026-10-20. The compile pass reads each class's fields (name, type, default) from syntax, as it reads system signatures. `declare` marks a field with no default, and TS emits nothing for it. The name defaults to `module#Class`, as Rust's comes from its module path, and `static path = "game::Health"` pins it, so renaming the file keeps saved levels working. A value is `Health.of({ hp: 3 })`, Rust's `Health { hp: 3.0, ..default() }`. Before: `class Health extends Component("game::Health", { hp: f32.default(100), max: f32.default(100) }) {}`, a name string plus a runtime schema object.

### A system with two queries and a resource

```rust
fn chase(
    time: Res<Time>,
    player: Single<&Transform, With<Player>>,
    mut enemies: Query<(&Enemy, &mut Transform), (With<Alive>, Without<Player>)>,
) {
    for (enemy, mut t) in &mut enemies {
        let dir = (player.translation - t.translation).normalize_or_zero();
        t.translation += dir * enemy.speed * time.delta_secs();
    }
}
```

```ts
export function chase(
  time: Res<Time>,
  player: Query<[Transform], With<Player>>,
  enemies: Query<[Enemy, Mut<Transform>], [With<Alive>, Without<Player>]>,
) {
  const [p] = player.single();
  enemies.each((enemy, t) => {
    dir.subVectors(p.translation, t.translation).normalize();
    t.translation.addScaled(dir, enemy.speed * time.delta_secs);
    enemy.speed = 3; // tsc error: Enemy is read-only in this query
  });
}
```

Before: one query per system (`app.system("chase", { stage, read, write, with, run })`) and no resources, so the player's position needed a second system and a component to carry it.

### Two queries over the same component

```rust
// Two queries that may both reach one Transform need a ParamSet (or Without<> filters).
fn separate(mut set: ParamSet<(Query<&mut Transform, With<Enemy>>, Query<(Entity, &Transform), With<Enemy>>)>) {
    let all: Vec<(Entity, Vec3)> = set.p1().iter().map(|(e, t)| (e, t.translation)).collect();
    for mut t in &mut set.p0() {
        if all.iter().filter(|(_, p)| p.distance(t.translation) < 1.0).count() > 1 {
            t.translation.y += 0.01;
        }
    }
}
```

```ts
// No ParamSet: the stage is exclusive and JS single-threaded, so both views alias one column.
export function separate(
  a: Query<[Entity, Mut<Transform>], With<Enemy>>,
  b: Query<[Entity, Transform], With<Enemy>>,
) {
  a.each((ea, ta) => b.each((eb, tb) => {
    if (ea !== eb && ta.translation.distance(tb.translation) < 1) ta.translation.y += 0.01;
  }));
}
```

Before: not expressible (one query per system).

### Ordering, run conditions, states

```rust
#[derive(States, Default, Clone, PartialEq, Eq, Hash, Debug)]
enum Game { #[default] Menu, Playing, Paused }

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct EnemySet;

app.init_state::<Game>()
    .add_systems(OnEnter(Game::Playing), spawn_wave)
    .add_systems(Update, (
        (spawn_wave, chase, separate).chain().in_set(EnemySet).run_if(in_state(Game::Playing)),
        apply_damage.after(EnemySet),
    ));

fn pause(mut next: ResMut<NextState<Game>>) {
    next.set(Game::Paused);
}
```

```ts
export class Game extends States("Menu", "Playing", "Paused") {}
export class EnemySet extends SystemSet {}

app.initState(Game.Menu)
  .addSystems(OnEnter(Game.Playing), spawnWave)
  .addSystems(Update,
    chain(spawnWave, chase, separate).inSet(EnemySet).runIf(inState(Game.Playing)),
    systems(applyDamage).after(EnemySet));

export function pause(next: NextState<Game>) {
  next.set(Game.Paused); // next.set("Paused") is a tsc error
}
```

`systems(a, b)` is Bevy's tuple `(a, b)`: functions cannot carry `.after()` in TS, so a group does. Before: no states, no run conditions, ordering by name string within a stage. `inState`, `resourceExists` and `onMessage` are data, so Rust evaluates them and a skipped system costs no JS call.

### A message and an observer

```rust
#[derive(Message)]
struct Damage { target: Entity, amount: f32 }

#[derive(EntityEvent)]
struct Explode { entity: Entity, radius: f32 }

fn apply_damage(mut damage: MessageReader<Damage>, mut q: Query<&mut Health>,
                mut score: ResMut<Score>, mut commands: Commands) {
    for d in damage.read() {
        let Ok(mut h) = q.get_mut(d.target) else { continue };
        h.hp -= d.amount;
        if h.hp <= 0.0 {
            score.value += 1;
            commands.trigger(Explode { entity: d.target, radius: 3.0 });
        }
    }
}

fn on_explode(explode: On<Explode>, q: Query<(Entity, &Transform), With<Enemy>>,
              mut damage: MessageWriter<Damage>, mut commands: Commands) {
    let Ok((_, at)) = q.get(explode.entity) else { return };
    for (e, t) in &q {
        if t.translation.distance(at.translation) < explode.radius {
            damage.write(Damage { target: e, amount: 50.0 });
        }
    }
    commands.entity(explode.entity).despawn();
}

fn on_health_added(add: On<Add<Health>>, mut q: Query<&mut Health>) {
    if let Ok(mut h) = q.get_mut(add.entity) { h.hp = h.max; }
}
```

```ts
export class Damage extends Message { declare target: Entity; declare amount: f32 }
export class Explode extends EntityEvent { declare radius: f32 }

export function applyDamage(damage: MessageReader<Damage>, q: Query<[Mut<Health>]>,
                            score: ResMut<Score>, commands: Commands) {
  damage.each((d) => {
    const [h] = q.get(d.target) ?? [];
    if (!h) return;
    h.hp -= d.amount;
    if (h.hp <= 0) {
      score.value += 1;
      commands.trigger(Explode.of({ entity: d.target, radius: 3 }));
    }
  });
}

export function onExplode(explode: On<Explode>, q: Query<[Entity, Transform], With<Enemy>>,
                          damage: MessageWriter<Damage>, commands: Commands) {
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
```

Before: none. TS-defined resources and events were a bridge non-goal; the only reactions were TSX `onClick` handlers.

### A plugin

```rust
pub struct EnemyPlugin;
impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Score>()
            .add_message::<Damage>()
            .init_state::<Game>()
            .add_systems(OnEnter(Game::Playing), spawn_wave)
            .add_systems(Update, (
                (spawn_wave, chase, separate).chain().in_set(EnemySet).run_if(in_state(Game::Playing)),
                apply_damage.after(EnemySet),
            ))
            .add_observer(on_explode)
            .add_observer(on_health_added);
    }
}
app.add_plugins(EnemyPlugin);
```

```ts
export function enemyPlugin(app: App) {
  app.insertResource(Score.of({ value: 0 }))
    .addMessage(Damage)
    .initState(Game.Menu)
    .addSystems(OnEnter(Game.Playing), spawnWave)
    .addSystems(Update,
      chain(spawnWave, chase, separate).inSet(EnemySet).runIf(inState(Game.Playing)),
      systems(applyDamage).after(EnemySet))
    .addObserver(onExplode)
    .addObserver(onHealthAdded);
}
app.addPlugins(enemyPlugin);
```

Before: none; a module called `app.system` at top level. A plugin is a function the loader re-runs on every generation.

### Spawning a prefab

```rust
// Components in a bsn! scene must derive Default and Clone.
fn grunt(at: Vec3, speed: f32) -> impl Scene {
    bsn! {
        Transform { translation: at }
        Enemy { speed: speed }
        Health { hp: 100.0, max: 100.0 }
        Alive
        RigidBody::Dynamic
        Collider::cuboid(0.5, 0.5, 0.5)
        on(|click: On<PointerClick>, mut commands: Commands| {
            commands.trigger(Explode { entity: click.entity, radius: 3.0 });
        })
    }
}

fn spawn_grunt(mut commands: Commands) {
    commands.spawn_scene(grunt(vec3(0.0, 0.0, -12.0), 3.0));
}
```

```tsx
export const Grunt = ({ at = [0, 0, 0], speed = 2 }: { at?: Vec3Like; speed?: f32 }) => (
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
Grunt.path = "game::Grunt"; // a .bsn level calls it as @game::Grunt { speed: 3.0 }

export function waves(commands: Commands, time: Res<Time>, s: Local<{ timer: f32; wave: u32 }>) {
  s.timer += time.delta_secs;
  if (s.timer < 5) return;
  s.timer = 0;
  s.wave += 1;
  for (let i = 0; i < s.wave; i++) commands.spawn(<Grunt at={[i * 2 - s.wave, 0, -12]} speed={2 + s.wave * 0.5} />);
}
```

A component class is its own TSX tag: `<Health initialHp={100} />` takes its props from the class fields. Before (tsx): the prefab was the same, spawning was `ctx.commands.mount(<Enemy at={p} />)`, and handlers were `onClick` on `<button>` only. `<entity onPointerClick>` is an entity observer, which is what `on(...)` is in BSN. `commands.spawn` takes component instances (`new Health({ hp: 3 })`) or a prefab element, so `mount` folds into `spawn`.

### A UI HUD, with and without hooks

```rust
#[derive(Component, Default, Clone)]
struct HpText;

fn hud() -> impl Scene {
    bsn! {
        Node { position_type: PositionType::Absolute, top: px(12), left: px(12) }
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5))
        Children [
            HpText
            Text("")
        ]
    }
}

fn update_hp(player: Single<&Health, (With<Player>, Changed<Health>)>,
             mut text: Single<&mut Text, With<HpText>>) {
    text.0 = format!("HP {} / {}", player.hp.ceil(), player.max);
}
```

tsx before the ruling, with a hook:

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

Ruled 2026-10-20, with no hook:

```tsx
export const Hud = ({ player }: { player: NodeRef }) => (
  <node style={{ position: "absolute", top: px(12), left: px(12) }} background="#0008">
    <text size={20}>{() => `HP ${Math.ceil(player.get(Health).hp)} / ${player.get(Health).max}`}</text>
    <text size={16} color={() => (res(Score).value > 10 ? "#fd0" : "#fff")}>{() => `Score ${res(Score).value}`}</text>
    <button onPointerClick={({ commands }) => commands.trigger(Explode.of({ entity: player.entity, radius: 5 }))}>
      <text>Boom</text>
    </button>
  </node>
);
```

Rust needs a marker component and a system per changing label. TSX says the label is a function of `Health` in place.

`player.get(Health)` inside `() => ...` is a tracked read. The site learns its reads on first evaluation, as a system learns its query (tsx decision 11). Rust re-runs the site only when that row's `Health` tick moves. This is decision 9's per-pair check, with no JS call in a quiet frame. Outside a binding, `player.get` logs a dev warning naming the location, because the value would never update.

Both TSX versions cost the same. The hook buys nothing the binding does not already know, and it adds a placement rule. It also hands agents the React idiom: `useComponent` is one step from `useState` and `useEffect`, which is how bevy-ts would become bevy-react.

A hook-like API is still the right tool in two places, and neither is named `use*`:

- `signal()`: UI-local state that is not game state, such as a dropdown's open flag or a text field's draft. Components run once, so a signal has no call-order rule.
- A declarative list over a query, for an inventory or a scoreboard: `<For each={query(Entity, Item).with(InBag)}>{(e, item) => ...}</For>`. It needs a subscription whose lifetime is the UI subtree. It is tsx's open `useQuery` question, answered with a query instead of a hook.

### Adopting Rapier, end to end

```toml
# Cargo.toml
bevy_rapier3d = "0.37"
```

```rust
app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
    .add_systems(Update, hurt)
    .add_systems(FixedUpdate, (steer, grounded).before(PhysicsSet::SyncBackend));

fn steer(mut q: Query<(&Enemy, &Transform, &mut Velocity)>, player: Single<&Transform, With<Player>>) {
    for (enemy, t, mut v) in &mut q {
        let dir = (player.translation - t.translation).normalize_or_zero();
        v.linear.x = dir.x * enemy.speed;
        v.linear.z = dir.z * enemy.speed;
    }
}

fn hurt(mut contacts: MessageReader<CollisionEvent>, mut q: Query<&mut Health, With<Player>>) {
    for c in contacts.read() {
        if let CollisionEvent::Started(a, b, _) = *c {
            let player = if q.contains(a) { a } else { b };
            if let Ok(mut h) = q.get_mut(player) { h.hp -= 10.0; }
        }
    }
}

fn grounded(rapier: ReadRapierContext, q: Single<(Entity, &Transform, &mut Velocity), With<Player>>) {
    let Ok(ctx) = rapier.single() else { return };
    let (e, t, mut v) = q.into_inner();
    let hit = ctx.cast_ray(t.translation, Vec3::NEG_Y, 1.1, true, QueryFilter::default().exclude_rigid_body(e));
    if hit.is_none() && v.linear.y > -20.0 { v.linear.y -= 0.5; }
}
```

```jsonc
// bevy-ts.json
{ "native": { "ports": ["rapier3d"] } }
```

```ts
import { CollisionEvent, PhysicsSet, type ReadRapierContext, Velocity } from "bevy-ts/rapier";

export function steer(player: Query<[Transform], With<Player>>, enemies: Query<[Enemy, Transform, Mut<Velocity>]>) {
  const [p] = player.single();
  enemies.each((enemy, t, v) => {          // v is Rapier's own Velocity column, zero-copy
    dir.subVectors(p.translation, t.translation).normalize();
    v.linear.x = dir.x * enemy.speed;
    v.linear.z = dir.z * enemy.speed;
  });
}

export function hurt(contacts: MessageReader<CollisionEvent>, players: Query<[Mut<Health>], With<Player>>) {
  contacts.each((c) => {
    if (c.kind !== "Started") return;
    const [h] = players.get(c.a) ?? players.get(c.b) ?? [];
    if (h) h.hp -= 10;
  });
}

export function grounded(rapier: ReadRapierContext, player: Query<[Entity, Transform, Mut<Velocity>], With<Player>>) {
  const [e, t, v] = player.single();
  const hit = rapier.castRay(t.translation, down, 1.1, true, { excludeRigidBody: e });
  if (!hit && v.linear.y > -20) v.linear.y -= 0.5;
}

app.addSystems(FixedUpdate, systems(steer, grounded).before(PhysicsSet.SyncBackend))
  .addSystems(Update, hurt);
```

The project writes no Rust. bevy-ts generates the native crate from `bevy-ts.json` and builds the project engine once per Rust change, or fetches it from the shared cache (native decisions 1 and 2). A port declares its own param kinds (`ReadRapierContext`) for the reader.

Before (native): a native crate with hand-written glue for every type TS should write: a `#[repr(C)]` mirror of `Velocity` with `#[derive(NativeLayout)]`, two systems copying it in and out around the step, a mirror message for contacts, a native function for the raycast, and `bevy_ts::native!` registering it all.

## Evidence, question 1: the TS surface

### What `tsc` proves (`bench/dx/`, `deno check`, TypeScript 6.0.3)

- **Query rows infer exactly.** `Query<[Entity, Enemy, Mut<Transform>, Option<Health>], With<Alive>>` gives rows of `[Entity, ReadView<Enemy>, Transform, ReadView<Health> | undefined]`. `examples/inference.ts` asserts each element with an exact type-equality check, and flipping one assertion fails the check. It is a mapped tuple type over the component classes; `tsc` needs nothing from bevy-ts's compile step.
- **Read and write are enforced at compile time.** Fields of a read component are `readonly`. A read vector is `ReadonlyVec3`, which has no mutating methods. A `Mut` vector field is a view: assignable by component (`t.translation.y += 1`) and by method (`t.translation.addScaled(v, s)`), but not reassignable (`t.translation = v` is an error). Misspelt fields are errors. The bridge spec today only enforces this at load.
- **Several queries per system** are several parameters, as in Rust.
- **Filters** are `With<C>`, `Without<C>`, `Changed<C>` and `Added<C>`, alone or as a tuple. `changed` reads bridge's per-row ticks against the system's last run (bridge decision 6). An `Or` filter is rare enough to defer.
- **Tiers show in the types.** `Query<[Mut<Collider>]>` gives rows of `never`, so any use is a type error (`Collider` is reflect tier), and `With<Collider>` is fine. States, messages and TSX props are typed too: `next.set("Paused")`, `<RigidBody value="Floating" />`, `<Transform translation={[1, 2]} />` and `onPointerClick` on `<mesh>` are all errors (`examples/tsx-errors.tsx`).
- **Not enforced by types:** giving both `hp` and `initialHp` (the load error stays), and a system that is not exported (a load error, below).

### ParamSet is not needed

Bevy needs `ParamSet` because two queries that may alias `&mut T` break Rust's borrow rules and stop the executor from running them in parallel. Neither applies to bevy-ts:

- The stage is exclusive and JS is single-threaded (bridge decision 1).
- Two queries over one column are two views of the same memory, so a write through one is visible through the other.

`separate` above reads and writes `Transform` through two overlapping queries; the Rust version needs a `ParamSet`. What does change is bookkeeping:

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

The API is glam-like and in place: every method writes `this` and returns it. Scratch vectors are made once with `vec3()` at module scope. Column views and scratch vectors share one interface, so `toPlayer.subVectors(a.translation, b.translation).normalize()` allocates nothing.

This is three.js's `Vector3` and gl-matrix's out-param style with glam's names. becsy, koota and bitECS ship no maths API; Thyseus had methods on component classes. Operators are out of reach: no JS engine overloads them.

### Bevy 0.20 names, checked in source

| Bevy 0.20 | Where | Proposed TS |
|---|---|---|
| `Message`, `MessageReader`, `MessageWriter`, `add_message` | `bevy_ecs/src/message/` | `class X extends Message { ... }`, `MessageReader<X>`, `MessageWriter<X>`, `addMessage` |
| `Event`, `EntityEvent`, `On<E>`, `commands.trigger` | `event/mod.rs`, `observer/system_param.rs` | `class X extends Event(...)` / `EntityEvent(...)`, `On<X>`, `commands.trigger` |
| `On<Add<C>>`, `Insert`, `Discard`, `Remove`, `Despawn` | `lifecycle.rs` (`EventPattern` for each) | `On<Add<C>>`, `On<Insert<C>>`, `On<Remove<C>>` |
| `On<PointerClick>` (not `Pointer<Click>`) | `bevy_picking/src/events.rs`, `examples/ui/navigation` | `onPointerClick`, `On(PointerClick)` |
| `in_state`, `on_message`, `run_if` | `bevy_state/src/condition.rs`, `schedule/condition.rs` | `inState`, `onMessage`, `systems(...).runIf` |
| `Local<T>`, `ParamSet` | `system/system_param.rs` | `Local<{ timer: f32 }>`; no ParamSet |
| `trait Resource: Component`; `ReflectResource` implies `ReflectComponent` | `resource.rs`, `reflect/resource.rs` | `class X extends Resource { ... }` is a component on a resource entity |

The last row matters most. In 0.20 a resource is a component on its own entity, so `Res<Score>` is a one-row column through the same path as a query. TS resources need no separate storage, and native's open question about "native resources as single-row views" answers itself.

### Hot-reload identity by `module#export`

- The loader walks each module's namespace and stamps every exported system, observer and plugin with `"src/enemy.ts#chase"`. That is the key for ordering, per-system `changed` ticks, message cursors and `Local` storage.
- `Local<{ timer: f32 }>` is a layout row kept in Rust under that key, so it survives reloads, as the hot-reload rules require of anything that must live.
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

None of them combines inferred tuples with declared mutability. Thyseus came closest with the same signature shape bevy-ts now uses; bevy-ts adds `Mut<T>`, reflect-tier errors in the types, and a reader that follows aliases and bundles and fails loudly.

### What the Bevy scripting crates do

- **bevy_mod_scripting 0.23:** a builder: `system_builder("name", script):query(world.query():component(A):with(C)):resource(R):after(other)`. Access is declared, nothing is typed, and access is through reflection per field.
- **bevy_mod_js_scripting:** abandoned. It used `world.query(Transform, Aabb)` with type names as strings.
- **bevy-react:** has no query API, only JSON messages.

The ruled surface is bevy_mod_scripting's declared-access model, written as Rust-style signatures, with the types Lua cannot have, over columns instead of reflection.

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

## What changes in the specs

### Applied: bridge's capability specs (ruled 2026-10-20)

Bridge is archived, so its capabilities live in `openspec/specs/`.

- `ts-systems`:
  - **Replaced:** `app.system(name, { stage, read, write, with, without, after, run })` gives way to exported functions with annotated params, registered with `app.addSystems(schedule, ...)`.
  - **Param kinds:** `Query<[data], filters>`, `Res<R>`, `ResMut<R>`, `Local<{ ... }>`, `Commands`, `MessageReader<M>`, `MessageWriter<M>`, `NextState<S>`, `State<S>`, plus kinds that ports declare.
  - **Access:** derived from query data, where `Mut<C>` writes, as the union over a system's queries. Several queries per system are allowed, overlapping included; there is no ParamSet.
  - **Filters:** `With`, `Without`, `Changed`, `Added`. Query data adds `Entity` and `Option<C>`.
  - **Iteration:** `q.each` is the hot loop, with `for...of`, `single`, `get`, `len` and `tables`.
  - **Schedules and ordering:** `Startup`, `Update`, `FixedUpdate` and the rest map onto the existing stages. Ordering is by function or set (`systems(...).after/.before/.inSet/.runIf`, `chain(...)`), not by name string.
  - **Run conditions:** `inState`, `resourceExists` and `onMessage` are evaluated in Rust.
  - **Fallback:** a setup closure, `system(() => { ...declare; return () => { ...run } })`, for computed param types.
- `script-runtime`: the oxc compile step gains the syntax-only annotation pass:
  - It follows aliases, generic aliases, bundles and imports.
  - It refuses computed types with the file and line.
  - A system registered without declarations throws.
  - The same pass ships as a vitest, Vite and esbuild plugin.
- `ts-components`: components, resources, messages and events are plain classes whose fields the compile pass reads (`class Enemy extends Component { speed: f32 = 2 }`), named `module#Class` unless `static path` pins it:
  - The class is the handle, the TSX tag and, through `Enemy.of({ ... })`, the constructor. Its instance type is the data.
  - Read views are `readonly`, and vector fields are in-place views with the glam-like method set.
  - `vec3()` and `quat()` make scratch values.
- `hot-reload`:
  - Systems, observers and plugins are identified by `module#export`, and an unexported system is a load error.
  - `Local` rows are kept in Rust under that identity and survive reloads.
  - Renaming an export is reported as a new system.
- **Moved into scope:**
  - *Resources:* components on resource entities, as in 0.20.
  - *Messages:* double-buffered arenas per type, with a cursor per system.
  - *Observers:* functions whose first param is `On<E>` or `On<Add<C>>`. A TS-triggered observer runs when its command applies; a Rust-triggered one runs at the next stage boundary.
  - *States:* `OnEnter`/`OnExit` run by one bevy-ts exclusive system in `StateTransition`.
  - *Plugins:* functions of `app`, re-run per generation.
- `commands.spawn` takes component instances or a prefab element.

### Applied: tsx has no hooks (ruled 2026-10-20)

- `useComponent` is removed. A binding reads `ref.get(C)` and `res(R)` directly; the read inside the binding is the subscription, checked per pair in Rust (decision 9) or per site (decision 11). There are no `use*` exports, and `signal()` stays for UI-local state.
- `useQuery` becomes the open question of `<For each={query(...)}>`.
- Edited: `openspec/changes/tsx` design (decision 9, goals, data model, demo, rulings), `reactive-ui` spec, tasks 5.3 and 5.6, proposal.

### Applied: reflection-derived layouts and one-line plugin adoption (ruled 2026-10-20)

- `openspec/changes/native`:
  - decision 5 is rewritten: tiers come from reflection, with the guards above;
  - decision 11 is rewritten: adoption by `native.ports`/`native.crates`, with curated ports;
  - N1 fills the registry from reflection;
  - the open question on resources is closed;
  - the rulings and tasks 1.1, 3.1, 3.2, 3.7 and 3b are updated.
- Specs edited:
  - `native-components`: the layout tier by reflection, and valid-value writes;
  - `native-crate`: plugins adopted in config;
  - `native-functions`: `NativeFn<name>` param kinds;
  - `openspec/specs/ts-components`: Rust layouts from reflection, so `Transform` is no longer special.

### Applied: the native review (operator, 2026-10-20)

A Fable review of the native change, checked against Bevy 0.20 and `bevy_rapier3d` 0.37 source:

- **TS calls Rust through exported system params, not `NativeFn<name>`.** `#[bevy_ts::export]` on a read-only `SystemParam`'s `impl` generates, per method:
  - a fast op;
  - the argument codecs;
  - the TS types;
  - the JS glue.

  A TS system takes `rapier: ReadRapierContext` and calls `rapier.castRay(...)`, as Rust does. A fixed record could not carry Rapier's `QueryFilter`.
- **The aliasing rejection is dropped.** Read-only access during the exclusive stage is sound, for the same reason overlapping queries are.
- **`bevy_reflect::func` is rejected.** It boxes every argument and cannot receive a `SystemParam`.
- **No `native!` macro.** The project's Rust is a plain Bevy `Plugin` named in `bevy-ts.json`, with an `App` extension trait (`ts_param`, `ts_set`, `ts_module`).
- **Unreadable leaves are holes.**
  - `Time`'s `Duration` fields are opaque to reflection (`impl_reflect_opaque!`), so the "every leaf plain data" rule would have refused `Res<Time>`.
  - Now `delta_secs` and `elapsed_secs` are columns, and the `Duration` fields are never seen.
- **Slots exist only for sets a generation names.** An exclusive system is a barrier even when skipped (`multi_threaded.rs:533`).
- **The Rapier port uses `in_fixed_schedule()`.** Rapier's default schedule is `PostUpdate`.
- **Shared cache: directory only for now.** The checksum proves integrity, not authenticity, and `bevy-ts dev` prints the build plan before compiling.
- **Script materials and the TypeGPU verdict moved to their own change**, `openspec/changes/script-materials`.


- **Spawning:** tsx B3 becomes `commands.spawn(<Prefab />)`; the systems spec already lets `spawn` take a prefab element.

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
- Reading types for runtime behaviour, checked 2026-10-20: TypeScript 7.0 announcement https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/ ; API roadmap microsoft/TypeScript#63875 and #64861 ; typia https://github.com/samchon/typia ; Deepkit `@deepkit/type-compiler` `compiler.ts` ; Thyseus `transformSystems.ts` ; oxc `decorator/legacy/metadata.rs` ; tsgolint https://github.com/oxc-project/tsgolint ; Angular inject() migration https://angular.dev/reference/migrations/inject-function ; TypeScript 5.0 decorators https://devblogs.microsoft.com/typescript/announcing-typescript-5-0/
- TS ECS libraries, read 2026-10-19: bitECS 0.4.0 https://github.com/NateTheGreatt/bitECS ; becsy 0.16.1 https://github.com/lastolivegames/becsy ; koota https://github.com/pmndrs/koota ; miniplex 2.0.0 https://github.com/hmans/miniplex ; Thyseus 0.18.0 https://github.com/JaimeGensler/thyseus (`packages/typescript-transformer`).
- Scripting crates: bevy_mod_scripting 0.23 `docs/src/ScriptSystems/introduction.md` (`~/dev/refs/bevy_mod_scripting`) ; bevy_mod_js_scripting https://github.com/jakobhellermann/bevy_mod_js_scripting ; bevy-react https://docs.rs/bevy-react.
- bevy-ts: `openspec/changes/{bridge,tsx,native,levels,viewport}`, `docs/research/change-detection.md`, `docs/research/scene-bindings.md`.
