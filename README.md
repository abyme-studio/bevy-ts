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
- **`bevy-ts dev src/game.ts`** launches the engine, loads the TypeScript and hot reloads it.
- **Custom materials without Rust**: a params schema in TS and the shader in a `.wgsl` file,
  hot reloaded ([docs/materials.md](docs/materials.md)).
- **Rust when it must be**: a project adopts Bevy plugins in one line of `bevy-ts.json`, or adds
  its own Rust as one Bevy plugin; bevy-ts then builds a project engine, only when Rust changes,
  cached by hash ([docs/native.md](docs/native.md)).

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
