## Why

The DX ruling of 2026-10-20 (docs/research/dx.md, main 7c1a813) rewrote `openspec/specs` for ts-systems, ts-components, script-runtime and hot-reload: TS systems are exported functions with Rust-style param types (`Query<[Enemy, Mut<Transform>], With<Alive>>`, `Res`, `ResMut`, `Local`, `Commands`, `MessageReader`/`Writer`, `NextState`), read by a syntax-only pass in the compile step, with `system(() => ...)` as the fallback; components, resources, messages, events and states are one class each; observers take `On<E>`; plugins are functions of the app; systems are identified by `module#export` and `Local` survives reloads. The code still implements the bridge's first surface (`app.system(name, { read, write, run })`, `component()`), so the specs and the code disagree. This change closes that gap over the bridge's manifest seam.

## What Changes

- **Compile step:** a syntax-only annotation reader in the oxc pass. It follows aliases (generic ones by substitution), param bundles, relative imports and port-declared param kinds, refuses computed types with file and line, and appends declarations and identities after each module's code, leaving every function body untouched.
- **Declarations:** `Component`/`Tag` classes, plus `Resource`, `Message`, `Event`/`EntityEvent` and `States`. A component class keeps the static `componentName` and `source` that the tsx JSX runtime keys on.
- **Systems:**
  - Multi-param runs with any number of queries (`Entity`, `Mut`, `Option`; `With`/`Without`/`Changed`/`Added`), resources as one-row columns, `Local` storage in Rust, `Commands`, message readers and writers, and `State`/`NextState`.
  - The query API: `each`, `for…of`, `single`, `get`, `len`, `tick`, `tables`.
  - Glam-style in-place vector methods, and `vec3()`/`quat()` scratch values.
- **Scheduling:**
  - `app.addSystems(schedule, ...)` covers `Startup`, the four stages, `OnEnter`/`OnExit`, `systems()`/`chain()` with `.after`/`.before`/`.inSet`/`.runIf`, and `systemSet`.
  - Run conditions (`inState`, `resourceExists`, `onMessage`, `not`) are evaluated in Rust, so a skipped system costs no JS call.
  - `Startup` runs once per identity.
- **Messages:** double-buffered, per-type arenas in Rust, read through a per-identity cursor.
- **Observers:** `On<E>`, `On<Add|Insert|Remove<C>>`, and `EntityEvent` propagation up `ChildOf`.
- **Plugins:** functions of the app, re-run every generation.
- **Identity:** `module#export` keys ordering, last-run ticks, message cursors and `Local` across reloads; `restart: true` resets them.
- **npm package and protocol:** the `bevy-ts` npm types move to the ruled surface (from `bench/dx/bevy-ts/mod.ts`), and the protocol's `bevy-ts/systems` reports params.
- **Tooling:** the same reader ships as Vite, vitest and esbuild plugins through `bevy-ts transform`, so systems load in tests outside the engine.
- **BREAKING:** `app.system`, `app.startup` and `component()` are removed for game code. The plugin-facing `section()`, `app.entry` and `Commands.custom` stay. Tests, the cube demo, the walk and the benchmark move to the new surface.

Not in this change (not yet ruled): reflection-derived layouts and one-line plugin adoption (`native`), and the no-hooks TSX binding.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `ts-systems`:
  - States what `Res<Time>` exposes.
  - Defines how `Changed`/`Added` rows reach JS.
  - Defines where observers triggered from TS and from Rust run relative to command application.
- `script-runtime`:
  - The declarations the compile step attaches, and how they bind to runtime values.
  - The `bevy-ts transform` command the tool plugins call.

## Impact

- Code:
  - `crates/bevy-ts`: manifest, systems, generation, commands, runtime.
  - `crates/bevy-ts-v8/js/core.js`, rewritten.
  - `crates/bevy-ts-cli`: compile, and the new `transform` subcommand.
  - `crates/bevy-ts-remote`: `bevy-ts/systems`.
  - `packages/bevy-ts`: types and tool plugins.
  - `examples/cube`.
- **tsx:** its JSX runtime keeps working (static `componentName`/`source`, `Commands.mount`). The tsx agent moves its types and tests after this lands.
- **Protocol:** `SystemInfo` gains `params` and `schedule`. That is a minor bump to 1.1.0, and the `read`/`write` fields are kept.
