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
beside it: a network share, or a CI artefact the team unpacks. On a local miss bevy-ts verifies the
shared engine's SHA-256 and only then runs it, so gameplay authors need no Rust toolchain. The
checksum proves the engine is intact, not who made it: whoever can write the cache can replace
both files, so point it only at a location the team controls.

The local engine (`.bevy-ts/engines/<hash>/bevy-ts`) is a symbolic link to the shared file, not a
copy. macOS scans every new executable path before its first launch, which takes a minute or
more; through a link every project on a machine runs one path per engine, so the scan is paid
once. `bevy-ts.link` beside the link records the SHA-256 and the target's size, modification time
and inode. When the target no longer matches, bevy-ts hashes it again and refuses it on a
mismatch. On Windows, or where a link cannot be made, the engine is copied as before. Engines
built with `cargo` and the engine `bevy-ts build` ships are always real files.

bevy-ts never writes under `engineCache`. Publish an entry by writing it to `<hash>.tmp/` and
renaming that to `<hash>/`, and never change an entry after: a rewrite is scanned again and can
change the binary under a running project. A link whose target is gone (the share unmounted, the
entry deleted) is a local miss: bevy-ts tries the shared cache again, then builds, which needs a
Rust toolchain.

## Rust components in TS

At startup the engine reads every registered reflected component's layout from reflection: it
takes a value (`Default`, `FromWorld` or the first live row) and records each field's offset as
the field's address minus the value's. No `repr(C)` and no code per type.

- **Layout tier**: a type with at least one plain-data field (`f32`, `f64`, integers, `bool`,
  `Entity`, glam vectors and quaternions, fieldless enums, arrays of numbers) is a zero-copy
  column in TS systems, with change ticks. Fields bevy-ts cannot read (`Duration`, `String`,
  nested structs) are holes: never visible to TS systems and never written by them. `Time` is not a column: each
  clock is a value built per run (docs/time.md). Enum variant bytes are read from the
  build, never assumed; `bool`, enum and `Entity` fields are written only through accessors.
  Components declared immutable are read-only. A type whose holes may own heap data is never
  inserted from bytes. A script `spawn` or `insert` starts from the type's `Default`, so its holes
  and the fields it leaves out keep their default (`AmbientLight.of({ brightness: 400 })` stays
  white).
- **Reflect tier**: a type with no plain-data field is typed in TS and reachable from TSX, the
  protocol and `With`/`Without` filters, never as query data.
- Components without `Reflect` can derive `bevy_ts_native::NativeLayout` (public fields, offsets
  from `offset_of!`) and register with `app.ts_layout::<T>()`.
- A tuple struct with one field (`ZIndex(i32)`, `TabIndex(i32)`, `Hovered(bool)`) names that
  field `value` in TS: `<ZIndex value={2} />`, `ref.get(Hovered).value`, `q.each((z) => z.value)`.
  A tuple struct with more fields keeps `"0"`, `"1"`. Level files and the protocol keep Bevy's
  reflection path (`.0`).

### Reflected values in TSX

TSX writes a reflected value in Bevy's own shape, through one writer (`bevy_ts::reflect_value`;
[reflection.md](reflection.md) has every kind's shape, its TS type and what is uncovered):

- An enum: a unit variant as its name (`"WindowSize"`), a variant with data as a one-key object
  (`{ Orthographic: { scale: 2 } }`); a tuple variant's data is an array, a newtype variant's its
  inner value. Nested enums work at any depth. A component that is itself an enum takes its whole
  value as `value`: `<Projection value={{ Orthographic: { scaling_mode: { FixedVertical: {
  viewport_height: 12 } } } }} />`.
- An `Option<T>`: `null` for `None`, a `T` for `Some`.
- A `Color`: any colour form (below).
- A written value is the JSON over the default: keys left out take the value in the component's
  `Default`, never the previous one, so dropping `scale` from a prop restores the default scale. A
  newly chosen variant starts from its default (a newtype variant from its inner type's `Default`
  or `FromWorld`); a struct or tuple variant with no default is given whole. An unknown key,
  variant or channel fails the load, naming it. A component with no `Default` merges over its
  current value. Removing a prop altogether (a hole prop, or `value`) keeps its last value, as
  for any controlled field. Types mark every struct field optional, so a struct variant given in
  part type-checks and fails the load when it needs to be given whole.
- A layout-tier component's holes that reflection can write (a `Color`, an enum with data, an
  `Option`, a nested struct) are TSX props: `<DirectionalLight illuminance={8000}
  color={hex("#ffd7a8")} />` writes `illuminance` from bytes and `color` through reflection, on
  insert and whenever it changes. TS systems still never see them. An opaque hole (`usize`,
  `Duration`) is written by its reflected serde; a hole of a type on the uncovered list fails the
  load with the list's reason; a hole prop takes a value, not a binding.

Colours have one parser (`bevy_ts::color`), used by every colour bevy-ts takes: TSX props, `<mesh
color>`, `<material color>`, UI colours, script-material `color` params and recipes. It takes a hex
string (`"#rgb"`, `"#rgba"`, `"#rrggbb"`, `"#rrggbbaa"`), `[r, g, b, a?]` in sRGB, or Bevy's
reflected `Color` (`{ Srgba: { red, green, blue } }`, `{ LinearRgba: ... }`, `{ Hsla: ... }`, every
variant), with `alpha` 1 when left out. `bevy-ts` exports `srgb(r, g, b, a?)`, `linear(r, g, b,
a?)`, `hsl(h, s, l, a?)`, `hex(s)` and the type `ColorValue`.

`bevy-ts types` builds the engine with no window and no GPU device, walks the type registry and
writes `.bevy-ts/types/native.d.ts`: layout-tier classes with their fields (and their writable
holes as TSX-only props), reflect-tier classes with their JSON shapes (`Color` as `ColorValue`,
nested struct fields optional, an enum component's `value`), and `NativeSet`, `NativeSchedule` and
`NativeCondition`, the unions of exposed set, schedule and condition names. Output is deterministic;
`bevy-ts dev` regenerates it after each project-engine build.

## Ordering against Rust

A crate exposes a Bevy set under a stable name, `app.ts_set("rapier:step", FixedUpdate,
PhysicsSet::StepSimulation)`; names are `<crate>:<name>` and unique. bevy-ts exposes its own
anchors: `bevy:transform-propagate` in `PostUpdate`, `bevy:fixed-main-loop` in
`RunFixedMainLoop` (the fixed steps), and one per stage, `bevy-ts:<stage>` in kebab case
(`bevy-ts:update`, `bevy-ts:fixed-last`), which names its set even before the stage exists. A TS system orders against one with
`systems(s).after(systemSet("rapier:step"))` (or `.before`) and runs in a slot around the set, in
the set's schedule. The first generation that names a set makes its slot: one exclusive system,
one schedule edit at that reload. Later reloads move systems between existing slots without
touching a schedule; an emptied slot stays and is skipped. An unused set has no slot, so it costs
the parallel executor nothing. A system names at most one exposed set, in that set's schedule,
and orders against other TS systems only within its slot. Rust orders against a slot with
`BevyTsSlot::before(name)` / `BevyTsSlot::after(name)`.

## Schedules, conditions and one-shot systems from Rust

A crate exposes a schedule label with `app.ts_schedule("net:tick", NetTick)`: TS adds systems
to it with `addSystems(schedule("net:tick"), sync)` and runs it with
`commands.runSchedule(schedule("net:tick"))`. A TS label is `bevy_ts::TsSchedule::new(path)` in
Rust, so a plugin can add systems to it or run it.

A crate exposes a run condition with `app.ts_condition("time:on-timer", factory)`. The factory
takes the JSON arguments of `nativeCondition("time:on-timer", { secs: 2 })` and returns a Bevy
`BoxedCondition`, or an error that fails the load with the call's location. It runs once per use
per generation; the instance is kept across reloads while the arguments stay the same, so its own
`Local`s and last-run tick survive. A port wraps it in a typed function. bevy-ts exposes Bevy's
time conditions this way, as `bevy:on-timer` and the rest, as it exposes the `bevy:` anchors;
scripts call the wrappers, `onTimer(2)` (`docs/time.md`).

```rust
app.ts_condition("time:on-timer", |args| {
    let secs = args["secs"].as_f64().ok_or_else(|| "on-timer takes { secs: number }".to_string())?;
    Ok(Box::new(IntoSystem::into_system(on_timer(Duration::from_secs_f64(secs)))))
});
```

Rust runs a TS one-shot system with `commands.queue(RunTsSystem::new("src/waves.ts#spawnWave"))`,
or with an `In<E>` input as `RunTsSystem::with(identity, bytes)`, the event's bytes in its layout.

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

Behaviour reaches TS as Bevy's own unit, a `SystemParam`. One attribute exports it:

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
naming the type. A `Result` error, a param that fails validation, or a panic throws in JS, naming
the param and method.

### Params that write

A param may write, and its methods may take `&mut self`:

```rust
#[derive(SystemParam)]
pub struct NavEdit<'w, 's> { mesh: ResMut<'w, NavMesh>, commands: Commands<'w, 's> }

#[bevy_ts_native::export(ts = "NavEdit", module = "bevy-ts/game")]
impl NavEdit<'_, '_> {
    pub fn block(&mut self, cell: u32) { self.mesh.block(cell) }
    pub fn mark(&mut self, at: Vec3) { self.commands.spawn((Marker, Transform::from_translation(at))); }
}
```

- In place: `ResMut`, `Option<ResMut>`, `ResMut<Assets<T>>`, `Query<&mut T>`, `Single`, `Populated`,
  `ParamSet`, `MessageWriter`, `MessageMutator`, `NonSendMut`. The write lands when the call
  returns; the system's next read of that column sees it.
- Deferred: `Commands`, `Gizmos`, `ParallelCommands`, any `Deferred<T>`. Nothing spawns, inserts or
  despawns during the run; columns never move under TS. The queue applies after the run, after
  the TS system's own commands, one param after another in the system's parameter order. A run
  that throws drops it, as it drops TS's commands.

```ts
export function build(nav: NavEdit, commands: Commands) {
  commands.spawn(new Tower());   // applies first
  nav.mark([0, 0, 0]);           // applies second, and sees the Tower
}
```

Every call is fetched at the calling system's own ticks and spends none. A native write carries
the run's tick, as a TS `Mut` write does: a Rust `Changed<T>` later in the frame sees it, and the
same system does not see its own write as a change next frame. `is_changed` on a read means
"since this system last ran". The dev warning "wrote to X, which no query of it declares Mut"
ignores columns an exported param of the system may write.

- One state per exported param, shared by every TS system that takes it: a `Local<T>` in it is
  shared, not one per system.
- A slice argument that views a column or resource row of the run (`q.tables[0].read(Health).f32`)
  throws when the slice is `&mut` or the param may write that column. Copy it (`.slice()`) first.
- Refused when `app.ts_param` runs, naming the param: anything that may write every component or
  resource (`Query<EntityMut>`, `EntityMutExcept`, `DeferredWorld`). Name the components and
  resources instead. `FilteredEntityMut` and `FilteredResourcesMut` take their access from a
  builder an export never has, so they reach nothing. `&mut World` and exclusive params are not
  `SystemParam`s and do not compile.
- A value TS already copied out of a column is stale after a native write, as a copied Rust value
  is.

### Standing for a Bevy spelling

An export may claim the `Res<..>` or `ResMut<..>` spelling it stands for, so TS writes Bevy's name
and never the export's:

```rust
#[bevy_ts_native::export(ts = "WindEdit", module = "bevy-ts/weather", stands_for = "ResMut<Wind>")]
impl WindEdit<'_> {
    pub fn gust(&mut self, strength: f32) { self.wind.gust(strength) }
}
```

```ts
export function storm(wind: ResMut<Wind>) { wind.gust(2); }
```

One export per spelling; a second claim fails `app.ts_param` naming both Rust types. Unclaimed
spellings read as before. `bevy-ts types` adds each claim to `ResClaims`/`ResMutClaims` in
`native.d.ts`, so `ResMut<Wind>` is typed as `WindEdit`, when TS declares the inner type (a class in
`native.d.ts`, or a clock: `ResMut<Time<Virtual>>` can be claimed by a `VirtualTime` export). A
claim whose inner type TS does not declare is listed in a comment there and not typed. Every
`Res<Time<..>>` stays the clock read docs/time.md describes, never a claim. The handshake lists the
claim as the param's `standsFor`.

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

Every engine ships `bevy-ts/input`: `Keys` (`pressed`, `justPressed`, `justReleased`) over
Bevy's `ButtonInput<KeyCode>`, `InputFocus` (`get()`, `undefined` for none) over Bevy's
`InputFocus`, and `KeyCode`, a TS enum generated from Bevy's variants (`KeyCode.KeyN`). They read
Bevy's own resources, so they agree with Rust systems. As in Bevy, a key typed into a field is
still pressed; check focus for a single-key shortcut:

```ts
import { KeyCode, type InputFocus, type Keys } from "bevy-ts/input";

export function openNotes(keys: Keys, focus: InputFocus) {
  if (keys.justPressed(KeyCode.KeyN) && focus.get() === undefined) open();
}
```

A number that is not a `KeyCode` throws, naming the param, the method and the number.

The other devices read the same way, each over Bevy's own state:

- `MouseButtons` (`pressed`, `justPressed`, `justReleased`) over `ButtonInput<MouseButton>`.
- `Cursor` (`position()`, `physicalPosition()`) over the window with `PrimaryWindow`: Bevy's
  `cursor_position()`, `undefined` while the cursor is outside it or there is no window. It is
  not `Window`, which TS already has as a reflected component.
- `Gamepads` over every `Gamepad`, the pad's entity first: `pressed`, `justPressed`,
  `justReleased`, `get` and `getUnclamped` (a button or an axis, `undefined` until the pad
  reports it), `leftStick`, `rightStick`, `dpad`, `vendorId`, `productId`. Find pads as in Bevy,
  with `Query<[Entity], With<Gamepad>>`; an entity with no `Gamepad` throws.
- `Touches` over `Touches`: Bevy's `anyJustPressed`, `justPressed(id)`, `getPressed(id)`,
  `firstPressedPosition` and the rest. Its iterators are `count()`/`at(i)`,
  `justPressedCount()`/`justPressedAt(i)` and the same for released and canceled, in an order that
  holds for the whole run. A touch is the record `{ id, position, startPosition,
  previousPosition, delta, distance }`, without Bevy's `force` (read `TouchInput` for it). Ids are
  winit's, as numbers.

`MouseButton`, `GamepadButton` and `GamepadAxis` are enums like `KeyCode`, without the `Other(n)`
variants. Their numbers never meet (`KeyCode` from 0, `MouseButton` from `0x1000`,
`GamepadButton` from `0x2000`, `GamepadAxis` from `0x2100`), so `gamepads.get(pad, ..)` takes
either gamepad enum, as Bevy's does, and a number of the wrong enum throws naming the one it is:

```ts
import type { Entity, Query, With } from "bevy-ts";
import { Gamepad } from "bevy-ts/bevy";
import { GamepadAxis, GamepadButton, MouseButton, type Cursor, type Gamepads, type MouseButtons } from "bevy-ts/input";

export function aim(mouse: MouseButtons, cursor: Cursor, gamepads: Gamepads, pads: Query<[Entity], With<Gamepad>>) {
  const at = cursor.position();
  if (at && mouse.justPressed(MouseButton.Left)) fireAt(at);
  pads.each((pad) => {
    if (gamepads.justPressed(pad, GamepadButton.South)) jump();
    steer(gamepads.get(pad, GamepadAxis.LeftStickX) ?? 0);
  });
}
```

Every public Bevy 0.20 input type, and where TS reaches it:

| Bevy type | TS | Owner |
|---|---|---|
| `ButtonInput<KeyCode>`, `KeyCode` | `Keys`, `KeyCode` | text-input |
| `InputFocus` | `InputFocus` | text-input |
| `ButtonInput<Key>`, `Key` (logical keys) | beside `Keys` | text-input, a later stage |
| `KeyboardInput`, `KeyboardFocusLost` | `MessageReader` | rust-events |
| `ButtonInput<MouseButton>`, `MouseButton` | `MouseButtons`, `MouseButton` | script-input-devices |
| `AccumulatedMouseMotion`, `AccumulatedMouseScroll`, `MouseScrollPixelsPerLine`, `MouseScrollUnit` | `Res<T>` | rust-resources; script-input-devices stage 2 |
| `MouseButtonInput`, `MouseMotion`, `MouseWheel` | `MessageReader` | rust-events; script-input-devices stage 3 |
| `Window::cursor_position`, `physical_cursor_position` | `Cursor` | script-input-devices |
| `CursorMoved`, `CursorEntered`, `CursorLeft` | `MessageReader` | rust-events; stage 3 |
| `Touches`, `Touch` | `Touches`, its touch record (no `force`) | script-input-devices |
| `TouchInput`, `TouchPhase`, `ForceTouch` | `MessageReader` | rust-events; stage 3 |
| `PinchGesture`, `RotationGesture`, `DoubleTapGesture`, `PanGesture` | `MessageReader` | rust-events; stage 3 |
| `Gamepad` | `With<Gamepad>` to find pads, `Gamepads` for its methods | script-input-devices |
| `GamepadButton`, `GamepadAxis`, `GamepadInput` | enums; `get` takes either | script-input-devices |
| `GamepadSettings`, `ButtonSettings`, `AxisSettings`, `ButtonAxisSettings` | read through reflection; no write | a follow-up |
| `GamepadEvent`, `GamepadConnectionEvent`, `GamepadButtonChangedEvent`, `GamepadButtonStateChangedEvent`, `GamepadAxisChangedEvent`, `RawGamepadEvent` and its parts | `MessageReader` | rust-events; stage 3 |
| `GamepadRumbleRequest`, `GamepadRumbleIntensity` | `MessageWriter` | rust-events; stage 3 |
| Rumble in viewport mode | none: the viewport protocol carries no rumble | a follow-up |
| `input_pressed`, `input_just_pressed`, `input_just_released`, `input_toggle_active` | `inputPressed` and friends | script-input-devices, on ecs-schedules' native conditions |
| Condition combinators | `not`, `and`, `or` | ecs-schedules |
| `ButtonState`, `Axis<T>` | through the params | none needed |
| `Gilrs`, `GilrsGamepads`, `RunningRumbleEffects`, `RumbleSystems` | none: backend internals | none |

Bevy has no gamepad condition: a pad's buttons belong to its entity, not to a `ButtonInput`.

`examples/rapier` uses all of it with no Rust in the project. Plans for leafwing-input-manager,
avian and bevy_hanabi are in [research/native-ports.md](research/native-ports.md).

The engine itself exports one param, from the built-in module `bevy-ts/camera`, in every
engine (`dev`, `play`, a shipped game, `bevy-ts types`). `Cameras` is Bevy's
`Camera::world_to_viewport`, so a UI node can sit over a world point:

```ts
import type { Cameras } from "bevy-ts/camera";

export function tag(cameras: Cameras) {
  const p = cameras.worldToViewport(camera, [0, 2, 0]);
  if (p) console.log(`the tag goes at left ${p.x}px, top ${p.y}px`);
}
```

- `worldToViewport(camera, point)` returns logical pixels from the viewport's top-left, the
  units of a node's `left` and `top` at the default `UiScale` of 1. A point in front of the camera but off screen returns
  coordinates outside the viewport; callers clamp or hide. It returns `undefined` when Bevy cannot
  project the point: behind the camera, past the far plane, or before the viewport has a size
  (the frame a camera spawns).
- `logicalViewportSize(camera)` is the viewport's logical size, `undefined` before it is known.
- `worldsToViewport(camera, points, out)` projects many in one host call: three floats per
  point in, two out (`NaN, NaN` for one that does not project), and returns how many projected.
  It throws when `points` is not a whole number of points or `out` is too short. Pass two
  separate arrays, not views of one buffer.
- An entity that is not a camera throws.

The camera's `GlobalTransform` is the one last propagated, in `PostUpdate`: from `Update` it is
the previous frame's, so a tag over a moving camera or entity lags a frame. A system ordered
after `bevy:transform-propagate` reads the current frame's transforms.
