# bevy-ts

A TypeScript application layer for Bevy: write a Bevy app in TypeScript and TSX, change it
without recompiling Rust.

- **Rust owns the one ECS world**: scheduling, rendering, physics, assets, native plugins. A
  precompiled engine binary, built once.
- **TypeScript says what exists and how it behaves**: components (registered as real ECS
  components with native layouts), systems, scenes, UI.
- **The bridge** runs an embedded JS runtime and crosses the boundary in batches (queries,
  command buffers, events), never once per component access.
- **TSX describes the world; Bevy runs it.** A custom JSX runtime and reconciler turn TSX into
  native entities on mount and on change, never per frame. Stable keys keep runtime state across
  hot reloads; `initial*` props are never reset by a reload. Built on BSN templates where they fit.
  UI elements, text fields and their events are in [docs/ui.md](docs/ui.md).
- **`bevy-ts dev src/game.ts`** launches the engine, loads the TypeScript and hot reloads it.
- **Game UI is bevy_ui in TSX**: `<node>`, `<text>`, `<image>`, `<button>` with Bevy's layout,
  grid, gradients and shadows, each prop writing one component ([docs/ui.md](docs/ui.md)).
- **Animated glTF models without Rust**: `<PlayAnimation clip="models/Fox.glb#Animation2" />`
  beside a `WorldAssetRoot` plays a clip, cross-fading when it changes; `paused` and `cull`
  park it at no cost per frame, `repeat="Never"` or `{ Count: 3 }` bounds it, and `Animations`
  reads its progress from a system ([docs/animation.md](docs/animation.md)).
- **Bevy's mesh primitives in TSX**: `<mesh shape="torus" />` and every other Bevy 0.20 shape,
  with Bevy's props and builder settings, typed per shape ([docs/meshes.md](docs/meshes.md)).
- **2D**: `<Camera2d />`, `<mesh2d>` and `<material2d>`, sprites from an image path, and sprite
  picking, each with a test ([docs/2d.md](docs/2d.md)).
- **Custom materials without Rust**: a params schema in TS and the shader in a `.wgsl` file,
  hot reloaded ([docs/materials.md](docs/materials.md)).
- **Bevy's time**: `Res<Time<Real>>`, `Res<Time<Virtual>>`, `Res<Time<Fixed>>`, and `Timer` and
  `Stopwatch` fields with Bevy's methods ([docs/time.md](docs/time.md)).
- **Audio**: `<AudioPlayer value="sounds/music.ogg" />` plays Ogg or WAV in a window, and
  `AudioSinks` from `bevy-ts/audio` pauses, mutes, seeks and moves it ([docs/audio.md](docs/audio.md)).
- **npm packages**: a script imports an ES module package by name; bevy-ts resolves it at compile
  time and ships it with the game ([docs/scripts.md](docs/scripts.md)).
- **Pointer handlers** on entities and UI; 3D meshes pick with `"meshPicking": true`, and a test
  clicks one under `--headless` ([docs/picking.md](docs/picking.md)).
- **Script networking**: `fetch`, `WebSocket` (`ws:` and `wss:`, TLS verified by the platform),
  `Blob`, timers, `URL`, `AbortController`, `process.env` and `process.argv` for scripts, deny by
  default through `net.allow` and `env` in `bevy-ts.json` ([docs/networking.md](docs/networking.md)).
- **`bevy-ts build`** makes a folder another person can run: the engine renamed after the game,
  its compiled scripts and assets, with progress as JSON events ([docs/build.md](docs/build.md)).
- **Rust when it must be**: a project adopts Bevy plugins in one line of `bevy-ts.json`, or adds
  its own Rust as one Bevy plugin; bevy-ts then builds a project engine, only when Rust changes,
  cached by hash ([docs/native.md](docs/native.md)). Every reflected kind has one JSON shape for
  TSX, scripts and the protocol ([docs/reflection.md](docs/reflection.md)), and scripts build, read
  and edit any reflected asset ([docs/assets.md](docs/assets.md)).

## How it grows

The operator's rules for what goes into the engine (2026-10-22/23):

- **Bevy's principles, code first.** No engine feature Bevy itself lacks and is not discussing:
  primitives are Bevy's mesh shapes built in code, never an engine-shipped cube. Check Bevy
  upstream before proposing one.
- **All of Bevy from TypeScript.** Every reflected shape, all of bevy_ui, every mesh primitive:
  not the part one game needed. A missing one is closed as its whole category, with a coverage
  table and a test that fails on any hole (`crates/bevy-ts-cli/tests/it/reflect_coverage.rs`
  for reflection). A native recipe with fixed parameters (`spawnNative("camera3d")`) is a gap,
  not a design.
- **No workaround for a foundational gap.** A game that needs what the engine lacks gets the
  engine feature, state of the art: scripts reach the network through the web's own APIs
  ([docs/networking.md](docs/networking.md)), never data tunnelled over the Bevy Remote Protocol.
  A JS-platform feature follows the JS runtimes' standards, and a new dependency is still asked
  first.

## Start

```sh
npx bevy-ts new my-game      # a project from the cube example, installed
cd my-game && npx bevy-ts dev src/game.ts
```

`npx bevy-ts examples` lists the examples; `new my-game --example arena` starts from another.
`new` pins bevy-ts at the version that ran it, installs with the package manager that ran it
(`--no-install` skips that) and refuses a directory that is not empty. No Rust toolchain needed.

abyme's Bevy studio talks to a running game over the Bevy Remote Protocol, which bevy-ts extends
(reload, component schemas, scene reconcile, entity events). That contract is the only coupling.
With `bevy-ts dev --viewport` the engine renders into the studio's panes, the game's and live
previews of the project's models by the game's own renderer ([docs/viewport.md](docs/viewport.md)).
The studio edits levels (`.bsn`) only through bevy-ts ([docs/levels.md](docs/levels.md)).

The JS runtime is V8 through `deno_core` on desktop (ruled 2026-10-19,
[docs/research/js-runtime.md](docs/research/js-runtime.md)). The bridge stays engine-neutral:
buffers in, a command buffer out, so QuickJS-ng can be a later backend for iOS and consoles, and
the web uses the browser's engine.

Open: whether TSX compiles to static scene data, runs dynamically, or both.
