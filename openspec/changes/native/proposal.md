## Why

`bridge` makes TypeScript the gameplay language and ships one precompiled engine on npm, but some work cannot or should not be TypeScript: physics and other solvers over huge entity counts, anything in Bevy's render world, systems that must run in parallel, third-party Bevy plugins, components holding heap data, and platform SDKs. Today a project that needs any of these has no path except forking bevy-ts. This change specs the escape hatch: an optional native crate per project, built into a project engine only when its Rust changes, with its components, systems and functions typed and reachable from TS, so gameplay still never waits on a Rust compile.

## What Changes

- A rule for what stays Rust, by class, with the TS-side handle each class gets (design.md, "What stays Rust").
- A project's optional native crate (`native/`, a normal Cargo library depending on the `bevy-ts` crate at the npm package's exact version) holding the project's own Rust as one plain Bevy plugin named in `bevy-ts.json`; no entry macro.
- Project engines: `bevy-ts dev` and `bevy-ts build` use the npm package's prebuilt engine when there is no native crate, and otherwise a project engine (the same engine binary statically linked with the native crate) built by `cargo` and cached by a content hash of the Rust inputs. TS edits never touch it. A shared engine cache (a directory; HTTP later) lets a team build once in CI so gameplay authors need no Rust toolchain.
- Rust components in TS, in two tiers decided per type from reflection (operator ruling 2026-10-20): any reflected component with plain-data leaves is a zero-copy column in TS systems whatever its `repr`, Rapier's `Velocity` and Bevy's `Time` included (unreadable leaves are holes TS never sees); the rest are typed in TS and reachable from TSX, the protocol and plugins through reflection (bridge decision 14), never as columns. `#[derive(NativeLayout)]` covers components without `Reflect`.
- Third-party plugins adopted by one line in `bevy-ts.json` (`native.ports`, `native.crates`), with no Rust in the project; bevy-ts publishes curated ports (Rapier first) for what reflection cannot reach: exported params, constructors, flattened messages, sets. Types come from a generated `.d.ts` the engine emits from its type registry.
- Ordering against Rust: a crate exposes Bevy system sets under stable names; a TS system says `systems(s).after(systemSet("rapier:step"))` and runs in a slot made the first time a generation names that set (one schedule edit then, none on ordinary reloads), because every exclusive slot is an executor barrier even when skipped.
- Exported system params: TS calls Rust behaviour the way a Rust system does, through a read-only Bevy `SystemParam` in its signature (`rapier: ReadRapierContext`, `rapier.castRay(...)`); `#[bevy_ts::export]` on the param's `impl` generates the host functions, codecs, TS types and JS glue. Synchronous, one call per method call, batched methods for bulk work. A new seam operation, engine-neutral.
- Script materials and the TypeGPU verdict moved to their own change, `script-materials` (2026-10-20).
- Out of scope, with room left: Rust hot patching (`subsecond`), dynamic library plugins, prebuilt engine flavours (for example physics included), TS-authored compute and render passes, TS-defined resources and events, a native crate per platform.

## Capabilities

### New Capabilities
- `native-crate`: the project's optional Rust crate as a plain Bevy plugin, plugin adoption in config, version pinning against the npm package, and what it may register.
- `project-engine`: when bevy-ts uses the prebuilt engine or builds a project engine, the content hash, the local and shared caches, and how `bevy-ts dev` behaves when Rust changes.
- `native-components`: exposing Rust components to TS in the layout tier and the reflect tier, and the generated `.d.ts`.
- `native-ordering`: named Rust system sets, slots made when first named, and how TS systems order against them.
- `native-functions`: exported system params, their methods, their access rules and their place in the seam.

### Modified Capabilities

None. `bridge` and `tsx` are not archived; what this change needs from `bridge` is listed in design.md, "Depends on bridge".

## Impact

- New crates: `bevy-ts-native` (the `App` extension trait, set exposure, exported params, the `export` and `NativeLayout` macros in `bevy-ts-native-derive`), and `bevy-ts-rapier`, the first curated port. `bevy-ts-cli` gains the project-engine build and cache, and a `bevy-ts types` command.
- The `bevy-ts` crate is published on crates.io in lock-step with the npm package; the native crate pins `=x.y.z`.
- A project with a native crate needs a Rust toolchain on whichever machine builds its engine, and nowhere else if the shared cache is used.
- `protocol/bevy-ts.openrpc.json` gains exposed set and exported param listings in `bevy-ts/hello` and `bevy-ts/schema`: a minor bump.
- No third-party plugin is added to the default engine.
