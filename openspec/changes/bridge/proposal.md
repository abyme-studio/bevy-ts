## Why

bevy-ts has a name, a runtime ruling (V8 through `deno_core`, engine-neutral bridge) and nothing else. Every later piece (TSX scenes, UI, the QuickJS-ng and web backends, abyme's Bevy studio) stands on two things that do not exist yet: the bridge that lets TypeScript declare real ECS components and run systems over them without a boundary crossing per component, and the Bevy Remote Protocol contract the studio reads a running game through. This change specs both, so the first code written is the core and not a prototype to throw away.

## What Changes

- A Bevy 0.20 plugin that owns one embedded JS runtime and registers TS-defined components as real ECS components with native layouts (`f32`, `u32`, `vec3`, ...), built at runtime with `ComponentDescriptor::new_with_layout`.
- TS systems, `app.system(name, { stage, read, write, run })`, that run over Bevy queries with component columns handed to JS as zero-copy typed-array views, field writes in place, and structural changes (spawn, despawn, insert, remove) through a binary command buffer.
- An engine-neutral seam: the core knows "load modules, describe declarations, run a system over these buffers, take back a command buffer". V8 via `deno_core` is the only backend in this change; QuickJS-ng and the browser's engine come later behind the same seam.
- `bevy-ts dev src/game.ts`: a precompiled engine binary that compiles TS on the host side, watches the sources and hot reloads them without rebuilding Rust, under explicit state rules (ECS state survives, JS module state does not, layout changes migrate by field name).
- A public Rust plugin API so feature crates (the coming TSX crate first) extend the bridge without changing it: manifest sections, host-invoked JS entries, plugin command kinds, component lookup and encoding by name, reflected access to Rust components without a native layout, and restart notification. `.tsx` compiles with the automatic JSX runtime from `bevy-ts`, and protocol methods from feature crates live in the same schema file.
- Chrome DevTools / VS Code debugging of TS systems over the V8 inspector, with source maps.
- The bevy-ts Remote Protocol: methods served next to stock `bevy_remote` (`bevy-ts/hello`, `bevy-ts/reload`, `bevy-ts/schema`, `bevy-ts/get_components`, `bevy-ts/mutate_components`, `bevy-ts/entities+watch`, ...), versioned in one OpenRPC file, `protocol/bevy-ts.openrpc.json`, from which both bevy-ts and abyme's studio generate their types. It is the only coupling to the studio.
- Out of scope, with room left for them: TSX scenes and the reconciler (the `tsx` change, which builds on the plugin API), UI, TS-defined resources and events, the QuickJS-ng and web backends, parallel JS systems, writing inspector edits back to TS source, viewport streaming and the asset list.

## Capabilities

### New Capabilities
- `script-runtime`: the embedded JS runtime, the engine-neutral seam, which thread it runs on, how TS is compiled and loaded, errors and debugging.
- `ts-components`: declaring components in TS, the native layout rules, registration as dynamic ECS components, and the Rust components exposed to JS with a declared layout.
- `ts-systems`: declaring systems, stages and ordering, zero-copy column views, change detection, and the command buffer.
- `hot-reload`: the `bevy-ts dev` CLI, generations, what state survives a reload, schema migration, and failed reloads.
- `plugin-api`: the Rust extension points feature crates use (manifest sections, host-invoked entries, command kinds, component lookup and encoding, reflected access, restart notification).
- `remote-protocol`: the versioned BRP contract bevy-ts serves on top of stock `bevy_remote`, and the schema file both sides generate types from.

### Modified Capabilities

None (no specs exist yet).

## Impact

- New Cargo workspace: `bevy-ts` (core plugin, engine-free), `bevy-ts-v8` (the `deno_core` backend), `bevy-ts-remote` (protocol methods), `bevy-ts-cli` (the `bevy-ts` binary). New npm package `bevy-ts` (the TS API, its types and the protocol types).
- Dependencies: `bevy` 0.20 (with `bevy_remote`), `deno_core` 0.412 (V8 with the sandbox off, the default), `oxc` 0.154 for TS stripping and source maps, `notify` for file watching, `typify` for protocol codegen.
- macOS builds that ship V8 need the `com.apple.security.cs.allow-jit` entitlement when notarized.
- abyme's Bevy studio consumes `protocol/bevy-ts.openrpc.json`; a major version bump there is a breaking change for it.
