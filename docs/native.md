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
- `bevy-ts build` builds the release engine.

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

`bevy-ts types` builds the engine with no window and no GPU device, walks the type registry and
writes `.bevy-ts/types/native.d.ts`: layout-tier classes with their fields, reflect-tier classes
with their JSON shapes, and `NativeSet`, the union of exposed set names. Output is deterministic;
`bevy-ts dev` regenerates it after each project-engine build.

## Ordering against Rust

A crate exposes a Bevy set under a stable name, `app.ts_set("rapier:step", FixedUpdate,
PhysicsSet::StepSimulation)`; names are `<crate>:<name>` and unique. bevy-ts exposes its own
anchors: `bevy:transform-propagate` and `bevy-ts:<stage>`. TS systems order before or after a
named set and run in a slot around it, made the first time a generation names the set, so an
unused set costs the parallel executor nothing.

## Ports

A port is a crate `bevy-ts-<name>` whose plugin adds the third-party plugin the way TS gameplay
needs it and supplies what reflection cannot. `rapier3d` adds `RapierPhysicsPlugin` in
`FixedUpdate` and exposes `rapier:sync-backend`, `rapier:step` and `rapier:writeback`.
