# Scripts

What systems can query and command, under Bevy's names: [ecs.md](ecs.md).

## Using npm packages

A script imports a package by name, as in Node or a bundler:

```ts
import { connect } from "@abyme/client/connect";
```

bevy-ts resolves every import when it compiles, not at run time. It rewrites each one to a
relative path to the file it found, so the engine, `bevy-ts play` and a shipped game load only
relative imports and carry no resolver.

### What resolves

- `node_modules` folders, walked up from the importing file.
- A package's `exports` and `imports` maps, and `main` when it has no `exports`.
- Scoped packages (`@scope/name`).
- Symlinks, followed to the real file: pnpm and workspace links (`workspace:*`) load the linked
  package's own files.
- `.ts` and `.tsx` in a package compile like your own files (types stripped).
- A package's relative import may leave the package (`../../kernel/src/protocol.ts`).
- Relative imports in your own code resolve the same way: `./x.js` finds `x.ts`, and `./x`
  finds `x.ts`, `x.tsx`, `x.js`, `x.mjs`, `x.jsx` or `x/index.*`.

Export conditions, in a package's order: `bevy-ts`, `import`, `module`, `default`, plus
`development` under `bevy-ts dev` and `production` under `bevy-ts build`. Not `node` (scripts
are not Node), not `browser` (there is no DOM), not `require` (ES modules only), not `types`
(no declaration files are loaded). A package can ship a `bevy-ts` condition for code meant for
games.

tsconfig `paths` are not read.

### What is refused

Each fails the load with the importing file and line:

- A package that is not installed.
- A subpath the package's `exports` do not offer.
- A Node built-in, `node:fs` or bare `fs`: scripts have no Node APIs.
- A CommonJS module: a `.cjs` file, or a `.js` file whose nearest `package.json` does not say
  `"type": "module"` and that has no `import` or `export`. bevy-ts loads ES modules only.
- A `.json` import.
- A package module that declares a component, resource, message, event, state or system set, or
  a `system(...)`: packages cannot declare ECS types yet. A package may import `bevy-ts` for
  anything else.

Package code is any module under a `node_modules` folder or outside the project root, so shared
code reached by a relative import or a linked folder outside the root cannot declare ECS types
either.

### Dynamic `import()`

A dynamic `import()` is not resolved when compiling. At run time it finds its target only among
the compiled modules, and otherwise rejects with an error the script can catch: a package name,
a file outside the graph, or a JSON import attribute. Import a package statically.

### `import.meta.url`

A module's `import.meta.url` is its file's URL: the real path under `bevy-ts dev`, and the
shipped file under `game/scripts/` in a built game. A URL that leaves a package finds nothing in
a shipped game.

### Installing

`bevy-ts dev` watches the `package.json` files it read and the nearest existing folder of each
path it looked for and did not find, inside the project or a package. Running `pnpm add` after a
"not installed" error reloads on its own, as does editing a linked package's `exports`.

### Shipping

`bevy-ts build` ships each package module at
`game/scripts/node_modules/<name>@<version>/<path in its package>` ([build.md](build.md)).

## Component fields

A field of a class extending `Component` is one of: `f32`, `f64`, `i8`, `u8`, `i16`, `u16`, `i32`,
`u32`, `bool`, `Vec2`, `Vec3`, `Vec4`, `Quat`, `Entity`, `Timer`, `Stopwatch`, a union of string
literals (an enum), `string`, or `T[]` with `T` a number kind, `bool`, `string`, a vector,
`Entity`, a union of string literals, or an object type of those but `string`. `string` and `T[]`
are heap fields (docs/ecs.md, "Heap fields"). Resources, messages, events and `Local`s take
the same types.

## Schedules, conditions and states

### Schedules

`app.addSystems(schedule, ...systems)` takes Bevy's schedules under their names: `PreStartup`,
`Startup`, `PostStartup`, `First`, `PreUpdate`, `RunFixedMainLoop`, `Update`, `SpawnScene`,
`PostUpdate`, `Last`, `FixedFirst`, `FixedPreUpdate`, `FixedUpdate`, `FixedPostUpdate` and
`FixedLast`, plus `OnEnter(...)` and `OnExit(...)`. Each schedule's TS systems run in one
exclusive bevy-ts stage, in set `BevyTsSet::<Stage>`. `PreUpdate`, `Update`, `PostUpdate` and
`FixedUpdate` have theirs from startup; any other schedule gets its stage the first time a
generation puts a system in it, so a schedule you never use costs nothing. A stage made by the
generation applied in `First` itself starts in the next frame. A system in a fixed schedule runs
once per fixed step and reads the fixed step as `Res<Time>`.

The three startup lists run when TS startup runs, at the generation apply, once per system per
world: every due `PreStartup` system, then `Startup`, then `PostStartup`. A reload runs only
systems it has not run before.

### Custom schedules

```ts
export class Waves extends ScheduleLabel {}
app.insertScheduleAfter(Update, Waves).addSystems(Waves, spawnWave);
```

A label is named like a component (`game::Waves`, or `static path`). In Rust it is
`bevy_ts::TsSchedule::new("game::Waves")`. `insertScheduleAfter/Before` place it in
`MainScheduleOrder`, `insertFixedScheduleAfter/Before` in `FixedMainScheduleOrder`,
`insertStartupScheduleAfter/Before` among the startup schedules. A reload that moves the call moves
the schedule from the next frame; one that drops it takes it out of the order. `app.initSchedule(Waves)` only creates
it. A crate's own schedule is `schedule("net:tick")`.

`commands.runSchedule(Waves)` runs a schedule once when the caller's commands apply, before the
next system of the stage. Running a schedule from inside itself logs an error and is skipped.

### One-shot systems

```ts
export function spawnAt(at: In<SpawnAt>, commands: Commands) { ... }
const id = app.registerSystem(spawnAt);
// in a system:
commands.runSystem(id);
commands.runSystemWith(spawnAt, SpawnAt.of({ x: 3 }));
```

A one-shot system runs when the caller's commands apply, before the next system. `In<E>`, its
input, is an `Event` class and the first param. It keeps its `Local`s and cursors as any system.
Rust runs one with `commands.queue(RunTsSystem::new("src/waves.ts#spawnAt"))`.

### Run conditions

Conditions are Bevy's, under their names, and are evaluated in Rust: a skipped system costs no JS
call.

| Condition | True when |
|---|---|
| `inState(Game.Playing)` | the state is that value |
| `onMessage(Damage)` | the system has unread messages |
| `runOnce()` | the first evaluation |
| `resourceExists(R)` | `R` exists |
| `resourceAdded(R)`, `resourceChanged(R)`, `resourceExistsAndChanged(R)` | `R` was added or changed since this condition last ran |
| `resourceRemoved(R)`, `resourceChangedOrRemoved(R)` | `R` stopped existing (or changed) since the last evaluation |
| `resourceEquals(R.of({...}))`, `resourceExistsAndEquals(...)` | `R`'s fields equal the value's |
| `anyWithComponent(C)` | some entity has `C` |
| `anyComponentRemoved(C)` | `C` was removed from some entity since the last evaluation |
| `anyMatchFilter(With(C), Without(D), Added(E), Changed(F))` | some entity matches every filter |
| `conditionChanged(c)`, `conditionChangedTo(c, true)` | `c`'s value changed (to `true`) |
| `onTimer(s)`, `onRealTimer(s)` | each time `s` seconds pass on the schedule's clock, or real time (`docs/time.md`) |
| `onceAfterDelay(s)`, `onceAfterRealDelay(s)` | once, `s` seconds in |
| `repeatingAfterDelay(s)`, `repeatingAfterRealDelay(s)` | every evaluation after `s` seconds |
| `paused()` | `Time<Virtual>` is paused |
| `not(c)` | `c` is false |
| `nativeCondition("crate:name", args)` | a condition a crate exposed (`docs/native.md`) |

Combine them with `a.andThen(b)`, `a.orElse(b)`, `a.nandThen(b)`, `a.norElse(b)`, `a.xor(b)`,
`a.xnor(b)`. Those that short-circuit skip `b` when `a` decides; `andEager`, `orEager`,
`nandEager`, `norEager`, `xor` and `xnor` always evaluate both, so a stateful `b` advances.
Each use of a stateful condition keeps its own tick or cursor. It survives a reload and a Stop
resets it.

`systems(a, b).runIf(c)` is Bevy's collective `run_if`: `c` is evaluated once per run of the
stage, before the group's first member, and holds for every member in that run.
`distributiveRunIf(c)` gives each member its own `c`.

A condition can also be an exported function returning `boolean`, a predicate:

```ts
export function bossReady(s: Res<Score>): boolean { return s.value >= 100; }
app.addSystems(Update, systems(spawnBoss).runIf(bossReady),
  systems(fight).runIf(resourceExistsAnd(Boss, alive)));
```

It goes wherever a condition goes: `runIf`, `distributiveRunIf`, `not` and the combinators.
Each evaluation is one call into JS, about 3 µs when the stage already runs TS (bench 3.1,
with a system of a predicate's shape standing in). In a stage
with no other TS system, the first call costs about 20 µs, the price of entering JS at all.
Put Rust conditions first: `inState(Game.Playing).andThen(bossReady)` makes no call outside
`Playing`. `resourceExistsAnd(R, pred)` calls `pred` only while `R` exists. A predicate reads
only: a `ResMut`, `Commands`, `MessageWriter`, `NextState` or `Mut<C>` param fails the load
with "a condition reads only; ResMut<Score> writes". A return that is not a boolean counts as
false and warns. When a predicate gates a single system, an early `return` in that system costs
the same.

### States

```ts
export class Game extends States("Menu", "Playing", "Paused") {}
app.initState(Game.Menu)
  .addSystems(OnEnter(Game.Playing), spawnLevel)
  .addSystems(OnTransition({ exited: Game.Paused, entered: Game.Playing }), resume)
  .addSystems(Update, systems(hud).runIf(stateChanged(Game)));
```

A TS state machine is a Bevy state. Each one takes a slot of the type `bevy_ts::TsState<N>`
the first time a generation declares it, so Rust reads `State<TsState<N>>`, writes
`NextState<TsState<N>>` and sees `StateTransitionEvent`s as for any state. An app has at most 32
state machines, sub and computed states included; a 33rd fails the load with "at most 32 state
machines". A value is its name, so reordering or adding values keeps the current one; a reload
that removes the current value moves the machine back to its `initState` value. A slot keeps its
depth (1 for a plain state, more for a sub or computed state) for the life of the app, so a reload
that changes a machine's depth, a plain state becoming a sub state, moves it to another slot: Rust
looks its slot up again with `TsStates::slot`.

Bevy runs the transitions in `StateTransition`, after `PreUpdate`: `OnExit(old)`, then
`OnTransition({ exited, entered })`, then `OnEnter(new)`, deepest machine first on exit. While the
game is paused (and in edit mode), pending transitions wait, and so does a new machine's first
`OnEnter`. `State<S>` reads the value. `NextState<S>` sets it: `next.set(v)` always transitions,
even to the current value, so `OnExit` and `OnEnter` run again and `DespawnOnExit` entities go;
`next.setIfNeq(v)` does nothing when `v` is the current value, as Bevy's `set_if_different`.
`PreviousState<S>` reads the value before the last transition. `stateChanged(S)` is true in the
frame a machine transitioned, `stateExists(S)` while it has a value.

`StateTransition` runs on Bevy's single-threaded executor: its many small systems cost far less
on one thread. That holds for Rust states too, so a heavy `OnEnter` system no longer runs beside
its siblings. A game that wants them in parallel opts back in after adding the plugin:

```rust
app.edit_schedule(StateTransition, |s| {
    s.set_executor(MultiThreadedExecutor::new());
});
```

State-scoped components are Bevy's own, run by Bevy's systems: `DespawnOnExit`, `DespawnOnEnter`,
`DisableOnExit`, `DisableOnEnter`, `EnableOnExit`, `EnableOnEnter`. Use them in commands
(`commands.spawn(Enemy, DespawnOnExit.of(Game.Playing))`), in TSX
(`<DespawnOnExit value={Game.Playing} />`) and in levels (`DespawnOnExit { value: "game::Game.Playing" }`).

```ts
export class Pause extends SubStates(Game.Playing, "Running", "Paused") {}
export class InGame extends ComputedStates(Game, (g) => (g === Game.Playing || g === Game.Paused ? InGame.Yes : undefined), "Yes") {}
```

A sub state exists only while its source is in that value, and enters at its first value; its
`NextState` is ignored while it does not exist. A computed state's value comes from its sources
(an array for several), `undefined` meaning it does not exist; its function is a JS call only when
a source transitions, and once after each reload. Its `NextState` is ignored with a warning. A
reload that removes a sub state's current value sends it to its first value. `PreviousState` of a
sub or computed state is cleared whenever it enters, leaves or is recomputed (Bevy gives no way to
write it for states it does not derive itself); it reads a value only after a sub state changes
through its own `NextState`.

A Rust state a crate exposes with `app.ts_state::<S>()` is a class in `bevy-ts/states`, its unit
variants its values: `import { EngineMode } from "bevy-ts/states"`, then `inState(EngineMode.Play)`,
`OnEnter(EngineMode.Edit)` or `NextState<EngineMode>`.

## Observers on one entity

`commands.entity(e).observe(fn)` attaches an observer to `e` alone, as Bevy's
`EntityCommands::observe`. `fn` is an exported function whose first param is `On<E>` of an
`EntityEvent`; it need not be passed to `app.addObserver`.

```ts
export function onHit(hit: On<Hit>, q: Query<[Health]>) {
  const row = q.get(hit.entity);
  if (row) row[0].hp -= hit.amount;
}

export function spawnDoor(commands: Commands) {
  commands.entity(commands.spawn(Health.of({ hp: 3 }))).observe(onHit);
}
```

- It runs only when the trigger's current target is `e`, bubbling hops included: an observer on a
  parent runs, with `entity` the parent, when a `static propagate = true` event bubbles up from a
  child. As in Bevy, global observers (`addObserver`) run first for each hop.
- `on.original` is the trigger's first target, the child in that case (Bevy's
  `original_event_target`). It is `undefined` for a global event, as `entity` is.
- The observers attached to a target are taken when the event is triggered, so triggering on `e`
  and despawning it in one system still runs them, as does `On<Remove<C>>` when `e` is despawned.
- It goes when `e` is despawned. A restart removes every one.
- It follows its function across a reload: edit `onHit` and the next hit runs the new body. If
  `onHit` is removed or renamed, its observers are removed with a warning. If its first param
  stops being an entity event, the reload fails, naming it.
- A function whose first param is not an entity event fails the run that observes it, and
  `tsc` refuses it.
- A reload that fails keeps the observers attached since the last drain; they run under the live
  generation.
- `on.event` is the event's whole value, as Bevy's `On::event()`: its fields, and for an entity
  event `entity`, the current target at each hop (the first is `original`).

## Rust messages and events

Bevy's messages and events come from `bevy-ts/bevy` and are used as TS ones are:

```ts
import { KeyboardInput, PointerClick, WindowResized } from "bevy-ts/bevy";

export function typed(keys: MessageReader<KeyboardInput>) {
  keys.each((k) => console.log(k.key_code, k.state, k.logical_key));
}
export function resized(r: MessageReader<WindowResized>) {
  for (const w of r) console.log(w.width, w.height);
}
export function clicked(click: On<PointerClick>) {
  console.log(click.entity, click.original, click.count);
}
app.addSystems(Update, typed, resized).addObserver(clicked);
```

- A message is a plain value in Bevy's reflected shape: enums as Bevy writes them
  (`"KeyA"`, `{ Character: "a" }`), vectors as arrays, a `Duration` in seconds.
- Each message is encoded for each reader that reads it, so the cost grows with the messages
  read (docs/native.md has the numbers). Read the types you need.
- Writing is `MessageWriter<M>.write(M.of(value))` or `commands.write(...)`; triggering is
  `commands.trigger(E.of(value))`. An enum's `of` takes its value whole:
  `GamepadRumbleRequest.of({ Stop: { gamepad } })`.
- A Rust event's observer runs at the next drain, once per bubbling hop. `propagate(false)` drops
  the rest of the hops TS would see; Rust observers have run already. For an event registered
  only with Bevy's `#[reflect(Event)]` it drops nothing.
- A Rust event field named `entity`, `original`, `propagate`, `toJSON` or `event` is read as
  `on.event.<name>`: `On` keeps its own. As Bevy's `On::event()`, `on.event`'s target is the
  current one at each hop: `click.event.entity` is `click.entity`, and `click.original` is the
  entity first clicked.
- A project's own message or event is registered in Rust (`app.ts_message::<Scored>()`,
  docs/native.md) and declared in TS by path, as a class with its fields:
  `export class Scored extends rustMessage("game_native::Scored") { declare points: u32 }`.

## Session bytes

A reload keeps the world and starts every module again, so a `let` at module level is lost on
each save. State that must outlive a save, and is keyed by id or too big for a component, goes in
session bytes: string keys to `Uint8Array` values, held in Rust (`bevy_ts::SessionBytes`, a
resource), not in the JS heap.

```ts
import { get, type SessionBytes } from "bevy-ts/session";

const buf = new Uint8Array(64);

export function remember(session: SessionBytes) {
  const pose = new Float32Array([0, 4, 10, 0.5]); // x, y, z, yaw
  session.put("heimdall/camera", new Uint8Array(pose.buffer));
  const camera = get(session, "heimdall/camera"); // a new Uint8Array, or undefined
  const n = session.read("heimdall/agent/42", buf); // into buf, no allocation
}
```

- `put(key, value)` stores a copy. `get(session, key)` returns a copy. `read(key, out)` copies into
  `out` when it fits and returns the value's length; when `out` is shorter, it copies nothing and
  still returns the length. `size`, `has`, `delete`, `clear`, `count` and `bytes` do what they say.
  For many keys a frame, `read` into one buffer.
- It survives every reload, failed ones too. A `restart: true` reload and a Stop empty it, as
  they reset `Local`s. It is never written to disk.
- One store for each app, shared by every module: prefix keys with your module (`"heimdall/..."`).
- A key is 1 to 1,024 bytes of UTF-8. The store holds at most 64 MiB, keys included. A `put` past
  either limit throws and changes nothing.
- Rust reads and writes the same store as `Res<SessionBytes>` / `ResMut<SessionBytes>`.
