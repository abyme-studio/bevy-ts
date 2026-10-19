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

abyme's Bevy studio talks to a running game over the Bevy Remote Protocol, which bevy-ts extends
(reload, component schemas, scene reconcile, entity events). That contract is the only coupling.

The JS runtime is V8 through `deno_core` on desktop (ruled 2026-10-19,
[docs/research/js-runtime.md](docs/research/js-runtime.md)). The bridge stays engine-neutral:
buffers in, a command buffer out, so QuickJS-ng can be a later backend for iOS and consoles, and
the web uses the browser's engine.

Open: whether TSX compiles to static scene data, runs dynamically, or both.
