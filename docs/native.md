# Rust in a bevy-ts project

Gameplay is TypeScript and reloads in the running engine. Some work cannot be TypeScript, and a
project brings Rust for it without forking bevy-ts. Gameplay still never waits on a Rust compile:
the engine is rebuilt only when Rust changes, and a team can share built engines.

## What stays Rust

| Class | Why not TS | Where it lives | What TS gets |
|---|---|---|---|
| Physics, solvers, collision | Pointer-heavy heap structures; SIMD and threads | An adopted plugin (Rapier) | Its plain-data components as columns, its sets for ordering, exported params for queries |
| Particles | GPU or parallel CPU systems | Native crate | Settings as columns |
| Spatial queries at scale | The index is a heap structure | Native crate, as a resource | Exported param methods, batched |
| Render-world work | Runs in the render sub-app, no JS there | Native crate | Settings as columns on cameras or entities |
| Parallel systems | JS runs exclusive on the main thread | Native crate | Ordering through slots, data through shared components |
| Third-party Bevy plugins | They are Rust crates | One line of `bevy-ts.json` | Their reflected types |
| Components with heap data (`String`, `Vec`, `Handle`) | No fixed layout; need drop | Rust types | Typed values in TSX and the protocol, filters in queries |
| Platform SDKs | C SDKs, foreign threads | Native crate | Exported params, components for results |
| Plain arithmetic over many entities | Nothing | TS systems | — |

## Adopting plugins and adding Rust

Everything is in `bevy-ts.json`:

```jsonc
{
  "native": {
    "ports": ["rapier3d"],                               // curated bevy-ts ports
    "crates": {                                          // any plugin crate on crates.io
      "bevy_kira_audio": { "version": "0.27", "plugin": "bevy_kira_audio::AudioPlugin" }
    },
      "crate": "native", "plugin": "my_native::GamePlugin" // the project's own Rust
  },
  "engineCache": "/mnt/team/bevy-ts-engines"             // optional shared directory
}
```

A plugin is a path to a unit struct or a `Default` type. The project's own Rust is a normal Cargo
library holding one Bevy plugin. It depends on `bevy-ts-native` at exactly the bevy-ts version and
on `bevy` at exactly the Bevy version bevy-ts was built with (Bevy's derives find `bevy` by name):

```toml
[dependencies]
bevy-ts-native = "=0.1.0"
bevy = { version = "=0.20.0", default-features = false }
```

A project with none of this runs the prebuilt engine from npm and never starts `cargo`.
`examples/rapier` adopts Rapier with no Rust; `examples/thrusters` has a native crate.

## Project engines

With anything under `native`, `bevy-ts dev` generates `.bevy-ts/engine/`, a binary crate whose
`main` runs the prebuilt engine's entry (`bevy_ts_cli::run`) with those plugins, and builds it.
The engine is keyed by a hash of its Rust inputs: the bevy-ts version, target, profile,
`features`, the `native` config, the native crate's `Cargo.lock`, workspace `Cargo.toml` and
every file of the crate and its path dependencies. TS, assets and the rest of `bevy-ts.json` are
not inputs, so a TS edit never rebuilds anything.

- A hit in `.bevy-ts/engines/<hash>/` starts at once.
- A miss tries the shared cache, then builds with `cargo`, printing the hash, why the caches
  missed and what will compile first. The first build compiles Bevy, rusty_v8 and the plugins
  (minutes); later Rust edits recompile only what changed.
- Before building, bevy-ts checks `Cargo.lock` resolves `bevy-ts-native` and `bevy` to its own
  versions and stops naming the crate, the expected and the found version.
- Under `bevy-ts dev`, a Rust save rebuilds in the background while the game runs. On success the
  engine restarts on the same port and loads the TS as a restart; on a compile error the old
  engine keeps running and cargo's errors are printed. World state does not cross the restart.
- `bevy-ts build` ships the project engine in the release profile inside a game folder
  ([build.md](build.md)).

### Sharing engines in a team

`engineCache` names a directory laid out as `<hash>/<target>/bevy-ts` with `bevy-ts.sha256`
beside it: a network share, or a CI artefact the team unpacks. On a local miss bevy-ts copies the
engine from there, verifies the SHA-256 and only then runs it, so gameplay authors need no Rust
toolchain. The checksum proves the copy is intact, not who made it: whoever can write the cache
can replace both files, so point it only at a location the team controls.

## Rust components in TS

At startup the engine reads every registered reflected component's layout from reflection: it
takes a value (`Default`, `FromWorld` or the first live row) and records each field's offset as
the field's address minus the value's. No `repr(C)` and no code per type.

- **Layout tier**: a type with at least one plain-data field (`f32`, `f64`, integers, `bool`,
  `Entity`, glam vectors and quaternions, fieldless enums, arrays of numbers) is a zero-copy
  column in TS systems, with change ticks. Fields bevy-ts cannot read (`Duration`, `String`,
  nested structs) are holes: never visible and never written. `Time` is a column for its
  `delta_secs` and `elapsed_secs` around its `Duration`s. Enum variant bytes are read from the
  build, never assumed; `bool`, enum and `Entity` fields are written only through accessors.
  Components declared immutable are read-only. A type whose holes may own heap data is never
  inserted from bytes.
- **Reflect tier**: a type with no plain-data field is typed in TS and reachable from TSX, the
  protocol and `With`/`Without` filters, never as query data.
- Components without `Reflect` can derive `bevy_ts_native::NativeLayout` (public fields, offsets
  from `offset_of!`) and register with `app.ts_layout::<T>()`.
- A tuple struct with one field (`ZIndex(i32)`, `TabIndex(i32)`, `Hovered(bool)`) names that
  field `value` in TS: `<ZIndex value={2} />`, `ref.get(Hovered).value`, `q.each((z) => z.value)`.
  A tuple struct with more fields keeps `"0"`, `"1"`. Level files and the protocol keep Bevy's
  reflection path (`.0`).

`bevy-ts types` builds the engine with no window and no GPU device, walks the type registry and
writes `.bevy-ts/types/native.d.ts`: layout-tier classes with their fields, reflect-tier classes
with their JSON shapes, and `NativeSet`, the union of exposed set names. Output is deterministic;
`bevy-ts dev` regenerates it after each project-engine build.

## Ordering against Rust

A crate exposes a Bevy set under a stable name, `app.ts_set("rapier:step", FixedUpdate,
PhysicsSet::StepSimulation)`; names are `<crate>:<name>` and unique. bevy-ts exposes its own
anchors: `bevy:transform-propagate` and `bevy-ts:<stage>`. A TS system orders against one with
`systems(s).after(systemSet("rapier:step"))` (or `.before`) and runs in a slot around the set, in
the set's schedule. The first generation that names a set makes its slot: one exclusive system,
one schedule edit at that reload. Later reloads move systems between existing slots without
touching a schedule; an emptied slot stays and is skipped. An unused set has no slot, so it costs
the parallel executor nothing. A system names at most one exposed set, in that set's schedule,
and orders against other TS systems only within its slot. Rust orders against a slot with
`BevyTsSlot::before(name)` / `BevyTsSlot::after(name)`.

## Edit mode and native systems

`bevy_ts::EngineMode` is a Bevy state, `Play` or `Edit`. It is `Edit` from the first frame under
`bevy-ts dev --edit` and follows every `viewport_mode` switch; a shipped game is always `Play`.
bevy-ts stops its own TS stages in edit mode, and `FixedUpdate` stops with `Time<Virtual>`. It does
not gate Rust systems it did not add: a native system that must not run in edit mode gates itself,
as any Bevy state does.

```rust
app.add_systems(Update, spin.run_if(in_state(EngineMode::Play)));
```

`OnEnter(EngineMode::Play)` and `OnExit(EngineMode::Play)` run when a client switches, before the
first play's `Startup`.

A Stop (play to edit) rebuilds the TS world from source (`docs/viewport.md`), but bevy-ts neither
resets nor despawns what Rust made: it can't know what a crate's resource means. A crate whose
state must not leak into the next Play resets it in `OnExit(EngineMode::Play)`, which runs before
the rebuild:

```rust
#[derive(Resource, Default)]
struct Waves(u32);

#[derive(Component)]
struct Spawned;

fn reset(mut waves: ResMut<Waves>, spawned: Query<Entity, With<Spawned>>, mut commands: Commands) {
    waves.0 = 0;
    for e in &spawned {
        commands.entity(e).despawn();
    }
}

app.add_systems(OnExit(EngineMode::Play), reset);
```

Components a port adds to TS entities, such as Rapier's bodies and colliders, go with their
entities: play, Stop, play runs the same.

## Calling Rust from TS: exported params

Behaviour reaches TS as Bevy's own unit, a read-only `SystemParam`. One attribute exports it:

```rust
#[derive(SystemParam)]
pub struct RapierQuery<'w, 's> { ctx: ReadRapierContext<'w, 's> }

#[derive(HostRecord)]
pub struct RayHit { pub entity: Entity, pub toi: f32 }

#[bevy_ts_native::export(ts = "ReadRapierContext", module = "bevy-ts/rapier")]
impl RapierQuery<'_, '_> {
    pub fn cast_ray(&self, origin: Vec3, dir: Vec3, max_toi: f32, solid: bool, filter: RayFilter) -> Result<Option<RayHit>, String> { ... }
    pub fn cast_rays(&self, rays: &[f32], max_toi: f32, solid: bool, filter: RayFilter, toi: &mut [f32], entity: &mut [f64]) -> Result<u32, String> { ... }
}
// in the plugin: app.ts_param::<RapierQuery<'static, 'static>>();
```

A TS system takes it as a parameter and calls it: `rapier.castRay(origin, [0, -1, 0], 100, true,
{ excludeRigidBody: e })`. Each call is one host call (a V8 fast op), synchronous inside the
system's run; slices are read and written in place, so `castRays` over 1,000 rays is one call.
Arguments and results are numbers, `bool`, `Entity`, `Vec2`, `Vec3`, `Quat`, `#[derive(HostRecord)]`
records, `Option`s of these, and `&[T]` / `&mut [T]` of numbers; anything else is a compile error
naming the type. A param that writes (`ResMut`, `Query<&mut T>`) does not compile. The param is
fetched read-only against the world, so a method sees the system's writes so far and cannot move
column memory. A `Result` error, a param that fails validation, or a panic throws in JS, naming
the param and method.

## Ports and built-in modules

A port is a crate `bevy-ts-<name>` whose `PortPlugin` adds the third-party plugin the way TS
gameplay needs it and supplies what reflection cannot: exported params, exposed sets, and a
built-in TS module registered with `app.ts_module("bevy-ts/<name>", js, dts)`. The module's
classes declare themselves with `__bevyts.data` and are declared by every generation;
`plugin_api::write_message` forwards Rust messages into a TS message type of the same name.
`bevy-ts types` writes the modules' declarations to `native-modules.d.ts` beside `native.d.ts`.

`rapier3d` adds `RapierPhysicsPlugin` in `FixedUpdate`, exposes `rapier:sync-backend`,
`rapier:step` and `rapier:writeback`, exports `ReadRapierContext`, and ships `bevy-ts/rapier`:

- `Collider`: Rapier's `Collider` has no `Reflect`, so `<Collider cuboid={[0.5, 0.5, 0.5]} />`
  sets the port's plain-data `ColliderShape`, which Rust turns into the real one; and
  `Collider.cuboid(commands, e, hx, hy, hz)` inserts one directly;
- `CollisionEvent`: Rapier's enum flattened to `{ kind, a, b, sensor }`, read with
  `MessageReader<CollisionEvent>` after `app.addMessage(CollisionEvent)`.

`examples/rapier` uses all of it with no Rust in the project. Plans for leafwing-input-manager,
avian and bevy_hanabi are in [research/native-ports.md](research/native-ports.md).
