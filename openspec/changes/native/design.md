## Context

`bridge` fixes the frame this change works inside: Rust owns the one world; TS components are dynamic components with C layouts; TS systems run in one exclusive system per stage on the main thread over zero-copy columns; Rust components cross into TS systems only with an `offset_of!` layout (decision 4), and everything else is reachable from plugins by reflection, never from TS systems (decision 14); feature crates extend the bridge through its Rust plugin API (decision 13); the engine ships prebuilt on npm in per-platform packages, so `npx bevy-ts dev` needs no Rust toolchain (ruling 2026-10-19). `tsx` adds scenes and UI on top, and its ruling puts levels in `.bsn` files.

What is missing is the other direction: the project that needs Rust. This design answers what can't or shouldn't be TS, and how a project adds Rust for it while gameplay iteration stays a TS reload.

Read for this design (2026-10-19):

- **Bevy 0.20 source** (`~/dev/refs/bevy-0.20`). `bevy_dynamic_plugin` is gone; `bevy_dylib` only speeds up linking. `ComponentDescriptor::new_with_layout` takes an optional drop fn (`bevy_ecs/src/component/info.rs:296`), so non-POD dynamic components are possible, but only from Rust that owns the drop. The render graph is now schedules (`RenderGraph` label, `Core3d` with `Core3dSystems` sets, `camera_driver`); custom passes are plain systems with `ViewQuery` and `RenderContext` (`examples/shader_advanced/custom_post_processing.rs`). Materials have an erased path: `MaterialProperties` (`bevy_material/src/lib.rs:53`) holds a runtime `BindGroupLayoutDescriptor`, shader handles and draw functions, and `examples/shader_advanced/manual_material.rs` builds its layout from `BindGroupLayoutEntries` and its bindings from `UnpreparedBindingResources` through `ErasedRenderAsset`, with no `AsBindGroup` derive. The bind group allocator is still keyed by one Rust `TypeId`. `AsBindGroup::bind_group_layout_entries` is a static fn, so a derived material has one layout per Rust type. The pipeline cache re-specializes on `AssetEvent<Shader>::Modified` (`pipeline_cache.rs:950,973`), so replacing a shader asset hot reloads it. `TypeRegistry::iter` with `StructInfo` gives field names and types; `bevy_remote`'s `registry.schema` exports JSON Schema (`builtin_methods.rs:1667`); reflection says nothing about `repr(C)` or offsets. The multithreaded executor runs an exclusive system alone and waits for everything running before it (`schedule/executor/multi_threaded.rs:533`).
- **TypeGPU** 0.12.8 (npm, 2026-10-16; MIT; 0.x with releases every one to three weeks; `unplugin-typegpu` 0.12.6, `tinyest` 0.3.4, `tgpu-gen` 0.12.0). `d.*` schemas: `bool`, `f16`, `f32`, `i32`, `u32`, `u16`, `vec2..4{b,f,h,i,u}`, `mat2x2f`/`mat3x3f`/`mat4x4f`, `struct`, `arrayOf`, `atomic`, pointer types, the loose `unstruct`/`disarrayOf`, attributes `align`/`size`/`location`/`builtin`; one WGSL alignment table (`vec3f` aligned 16), not separate std140/std430 rules (`data/alignmentOf.ts`). CPU read and write into an `ArrayBuffer` needs no device (`readFromArrayBuffer`, `writeToArrayBuffer`, on `typed-binary`). `tgpu.resolve` produces WGSL with no `GPUDevice` and no `navigator.gpu` (`core/resolve/tgpuResolve.ts`; `apps/bun-example` prints WGSL headless). JS-bodied shader functions (`'use gpu'`) need the bundler plugin to embed a `tinyest` AST at build time, or they throw "Missing metadata for tgpu.fn function body" (`core/function/fnCore.ts`); WGSL-template functions work without it. Resolution returns `code` plus `usedBindGroupLayouts`, but nothing maps those to a foreign wgpu host; no Rust or Bevy user is known. Hosts beyond the browser: React Native through `react-native-webgpu`, Bun.
- **Third-party plugins on crates.io, 2026-10-19.** On Bevy 0.20: `bevy_rapier3d` 0.37, `bevy_egui` 0.43.1, `bevy_kira_audio` 0.27.1, `leafwing-input-manager` 0.22. Still on Bevy 0.19: `avian3d`/`avian2d` 0.7.0 and `bevy_hanabi` 0.19.0, both last released June 2026. Plugins lag Bevy by weeks to months, and an engine pinned to one Bevy version can only take the plugins that have caught up.
- **Dynamic Rust plugins.** `bevy_dynamic_plugin` was deprecated in 0.14 and removed in 0.15 (bevyengine/bevy#14534) as unsound: no stable Rust ABI, and a loaded plugin must match the exact Bevy build and rustc. `abi_stable` is unmaintained (0.11.3, 2023); `stabby` is active. Bevy 0.17's `hotpatching` feature (`subsecond`, `dx serve --hot-patch`) patches only the binary crate, not Wasm, and does not reload a system whose parameters change; the feature still exists in 0.20 with no news since.
- **Godot GDExtension.** A `.gdextension` file maps each platform to a shared library and names an `entry_symbol`; classes registered through ClassDB appear in the editor and GDScript; bindings are generated from `extension_api.json`; `reloadable = true` since 4.2, with godot-rust's book advising to turn it off because reloads crash or lose state (gdext #543, #908). Godot still calls the API experimental.
- **Unity.** Native plugins are `[DllImport]` libraries per platform, linked statically (`"__Internal"`) on iOS under IL2CPP. Burst compiles a C# subset through LLVM for jobs, Unity's in-language fast path that keeps hot loops out of C++.
- **Unreal.** C++ modules expose types to Blueprint through UHT-generated reflection (`UFUNCTION`, `UPROPERTY`); Live Coding patches C++ in a running editor, without updating existing instances' constructor defaults.
- **Native binaries on npm.** esbuild, Biome, swc and Turbo all ship one package per platform in `optionalDependencies` with `os`/`cpu` fields. napi-rs does the same and prefers a local build in development; `node-gyp` builds from source when no prebuilt matches. No npm tool found builds a custom native binary from a user's own Rust.

## Goals / Non-Goals

**Goals:**
- A rule per class of work for what stays Rust, and the TS-side handle each class gets.
- A project adds Rust in one crate and one entry; the engine rebuilds only when Rust inputs change; TS reloads never wait on Rust.
- A team where only one machine (or CI) has a Rust toolchain.
- Rust components typed in TS, as columns when their layout allows it, as reflected values otherwise.
- TS systems ordered against Rust systems without rebuilding schedules on reload.
- Custom materials without Rust.
- A verdict on TypeGPU.

**Non-Goals:**
- Hot patching Rust in a running engine (`subsecond`, Bevy's hotpatching feature). Room is left: a Rust edit already restarts cleanly.
- Loading Rust from dynamic libraries, GDExtension-style.
- Prebuilt engine flavours with third-party plugins included (see Open Questions).
- TS-authored compute passes, render passes or render-world code of any kind.
- TS-defined resources and events (still out of scope from `bridge`); native resources visible to TS.
- More than one native crate per project, or per-platform native crates.
- Packaging and signing shipped games (`bevy-ts build` only gets the project-engine step here).

## What stays Rust

A class stays Rust when TS on one main-thread isolate cannot do it at the cost the frame allows, or cannot reach it at all. JS over typed arrays is not the bottleneck by itself: bridge's columns make a 1M-row numeric loop a JIT loop. What disqualifies TS is the shape of the work.

| Class | Why not TS | Where it lives | What TS gets |
|---|---|---|---|
| Physics, solvers, collision | Broadphase trees, contact graphs and islands are pointer-heavy heap structures, not columns; solvers want SIMD and threads | A third-party plugin (avian, rapier) in the native crate | Layout-tier components it reads (velocities, forces); slots `systems(s).after(PhysicsSet.StepSimulation)`; batched native functions for raycasts and shape casts |
| Particles | Simulated on the GPU (bevy_hanabi) or in parallel CPU systems | Native crate | Spawner settings as layout-tier components; effect assets by path |
| Spatial queries at scale (neighbours, navmesh, pathfinding) | The index is a heap structure; per-entity queries from JS would cross per call | Native crate, as a resource | Batched native functions, one call per batch |
| Render-world work: render-graph systems, passes, compute, readback, custom draw | Runs in the render sub-app on the render thread, with no JS and no main-world access | Native crate | Settings as layout-tier components on cameras or entities, extracted by Rust; materials are the exception below |
| Custom materials | Not disqualified: a material is WGSL plus parameters, which is data | Prebuilt engine's script-material crate | `material()` declarations, TSX `<material>` |
| Parallel systems | JS is exclusive on the main thread (bridge decision 1) | Native crate | Ordering through slots; data through shared components |
| Third-party Bevy plugins | They are Rust crates, compiled in | Native crate | Their reflected types (reflect tier), their sets if the crate exposes them, their components with `NativeLayout` if the crate author or a wrapper adds it |
| Components with non-POD data (`String`, `Vec`, `Handle<T>`, `Box<dyn T>`, maps) | No fixed layout, need drop and clone in Rust | Native crate, as Rust types | Reflect tier: typed values in TSX and the protocol, filters in queries; `Entity` refs are plain layout-tier fields |
| Platform integrations (Steamworks, console SDKs, IAP, native dialogs, haptics) | C SDKs, callbacks on foreign threads, platform build rules | Native crate | Native functions for requests, components or entries for results |
| Hot loops over huge counts that are plain arithmetic (boids, bobbing, damage over time) | Nothing: these stay TS | TS systems | — |

## Decisions

### 1. The escape hatch is one native crate, statically linked into a project engine
A project names one Cargo library in `bevy-ts.json`. bevy-ts generates `.bevy-ts/engine/`, a binary crate whose `main` is the prebuilt engine's own `main` with the native crate's entry passed in, and builds it with `cargo`. The project engine is the prebuilt engine plus the crate, nothing else: same CLI flags, same protocol, same TS loader.

- *Beat: dynamic libraries loaded at runtime (GDExtension, Unity native plugins, Bevy's removed `bevy_dynamic_plugin`).* Rust has no stable ABI. A dylib sharing Bevy types with the engine must be built by the same compiler against the same Bevy build with the same features, which is exactly a project-specific build anyway, plus unsoundness when anything drifts; Bevy removed its dynamic plugin loader in 0.15 for that reason (#14534), and godot-rust's own book advises turning GDExtension reload off. GDExtension avoids it by putting a C ABI over the whole engine, which for Bevy would mean wrapping every plugin's API in C.
- *Beat: a C ABI plugin surface (`abi_stable`, `stabby`).* It would let one prebuilt engine load plugins, but third-party Bevy plugins are written against Bevy's Rust API and would not cross it. It solves the toolchain problem only for code written specifically for bevy-ts.
- *Beat: letting the native crate own `main`.* Then every project carries engine startup code that must track bevy-ts releases. A generated `main` keeps the engine one thing.

### 2. Rebuild only when Rust inputs change, keyed by a content hash, with a shared cache
The engine hash covers the bevy-ts version, target, profile, engine features, `Cargo.lock`, the workspace's `Cargo.toml`s and the native package's files; never TS or assets. A hit in `.bevy-ts/engines/<hash>/` starts without `cargo`. A miss tries the shared cache (a directory or an HTTP URL template), verifying a SHA-256, then builds. CI runs `bevy-ts engine publish` per target. That is how "no Rust recompile for gameplay" holds for a team: gameplay authors fetch the engine their checkout needs, and only the people editing `native/` compile.

- *Beat: always `cargo build` and let Cargo's own freshness check decide.* Correct, but it requires a toolchain on every machine and pays Cargo's resolution on every launch.
- *Beat: committing engine binaries to the repository (or LFS).* It works with no service, but binaries per platform per change bloat history and drift from the source that built them; a hash-keyed cache cannot drift.
- *Beat: a bevy-ts hosted build service.* It is infrastructure and someone pays for it; a URL template lets a team point at any bucket it already has.

### 3. Rust edits restart the engine; they never patch it
Under `bevy-ts dev`, a save in the native crate rebuilds in the background while the old engine runs. On success the new engine starts on the same port and the TS generation loads as a restart (startups re-run, TSX remounts). On failure the old engine keeps running. World state does not cross the restart; Rust edits are rarer and coarser than TS edits, and the studio reconnects through `bevy-ts/hello`.

- *Beat: `subsecond` hot patching (Bevy's `hotpatching` feature).* It keeps state across Rust edits, but it patches only the binary crate (the native crate is a library), cannot reload a system whose parameters changed, cannot change layouts or registrations, and needs `dx serve` in the loop. Room left: a later change can patch when the edit qualifies and restart otherwise.
- *Beat: carrying world state across the restart through reflection.* TS components would serialise, but Rust components without `Reflect` would not, and the result would be a partial world that looks whole. Levels in `.bsn` (tsx ruling) are the durable state.

### 4. Version lock-step: the native crate pins bevy-ts and Bevy exactly
`bevy-ts-native` is published to crates.io at the npm package's version, re-exports `bevy`, and the native crate depends on it with `=x.y.z`. bevy-ts reads `Cargo.lock` before building and refuses a mismatch. A minor bevy-ts release that moves Bevy forces the project's third-party plugins to move with it, as any Bevy app does.

- *Beat: a semver range.* Two bevy-ts patch releases can carry different engine internals that the native crate links against; with a generated `main` linking the same crates, only an exact match is known to work.

### 5. Rust components cross in two tiers: layout and reflect
A `#[repr(C)]` component of bridge field types derives `NativeLayout`, which fills bridge's layout registry from `offset_of!` (bridge decision 4 made public). It is a zero-copy column in TS systems, with change ticks, exactly as `Transform` is. Every other registered component is reflect tier: typed in TS, usable in TSX elements, the protocol and plugins through bridge decision 14, and in `with`/`without` filters, never in `read`/`write`. Non-POD data (`String`, `Vec`, `Handle`, `Box<dyn>`) is reflect tier by construction. A component that needs both is split into two components on one entity.

- *Beat: reading `repr(C)` and offsets from reflection.* Reflection carries neither (checked in Bevy 0.20); guessing offsets is what bridge decision 4 forbade.
- *Beat: requiring `repr(C)` for every component TS sees.* Most third-party components are not, and the reflect tier is cheap for components that change rarely.
- *Beat: TS components with heap fields via a drop fn.* `new_with_layout` accepts a drop fn, but the data would be opaque bytes to TS anyway; the owner of heap data must be Rust.

### 6. TS types come from the engine's registry, generated, not hand-written
`bevy-ts types` builds the engine's `App` with all plugins added, stops before the renderer creates a device, walks `AppTypeRegistry` for components registered for TS, exposed sets and native functions, and writes `.bevy-ts/types/native.d.ts`. Layout-tier fields map to bridge field types; reflect-tier fields follow `registry.schema`'s JSON Schema. The prebuilt engine's types ship in the npm package as `bevy-ts/bevy`. Output is deterministic and diffed in CI, the way bridge's protocol types are.

- *Beat: hand-written `declare` blocks.* They drift on the first Rust edit.
- *Beat: a proc macro writing TS at compile time.* It sees one crate at a time, so it misses third-party plugins' reflected types and anything registered at runtime.
- *Beat: generating from the `bevy-ts/schema` protocol method of a running engine.* Same data, but needs a window and a running game; the registry walk can run in CI with no GPU.

### 7. TS orders against Rust through named sets with fixed slots
The native crate exposes a system set under a name (`physics:step`). At app build the engine adds two exclusive stage systems around it, a before slot and an after slot. TS systems say `before`/`after` with that name and run in that slot; an empty slot is skipped by a run condition. Rust orders against TS through `BevyTsSet` and the slot sets. The schedule is fixed by the engine binary, which only changes when Rust changes, so a TS reload still never rebuilds a schedule (bridge decision 1 holds).

- *Beat: one Bevy system per TS system, inserted against the Rust set on each reload.* Precise, but it rebuilds schedules on every save, the cost bridge decision 1 refused.
- *Beat: only the four bridge stages.* A TS system reacting to physics results would run a frame late, or the author would have to move it to Rust.
- *Beat: slots around every Rust system set automatically.* Each exclusive slot is a barrier for the parallel executor; slots must be opted into by name, and empty ones skipped.

### 8. Native functions: batched, synchronous, read-only, declared
A native function is Rust taking a read-only `SystemParam` and a batch of typed records, returning a batch of typed records into a Rust-owned buffer viewed by JS. A TS system lists the functions it `uses`; load rejects a system whose `write` set overlaps what a function reads, so a function never reads a column JS is writing. Structural changes go through the system's command buffer. This is a new seam operation, JS calling a host function, which `deno_core` (ops), QuickJS-ng (host functions) and the web (wasm imports) all have.

- *Beat: requests as commands, results next frame.* Engine-neutral without a new seam op, but a frame of latency for every raycast changes gameplay.
- *Beat: per-call functions without batches.* One crossing per ray; bridge's rule is crossings per run, not per item.
- *Beat: giving native functions `&mut World`.* JS holds live column views during the run; a mutating function could move or free the memory under them.

### 9. TypeGPU: borrow ideas, do not adopt
Verdict: skip TypeGPU as a dependency; borrow its schema-per-WGSL-type idea for script-material params and its headless `resolve` as the model for generating WGSL declarations from a TS schema.

- **As the component layout vocabulary: no.** Its `d.*` types encode WGSL alignment (`vec3f` aligned 16, struct sizes rounded to 16), and components must follow C rules so they match Rust's `Vec3` (align 4) and `offset_of!` layouts of Rust components. Using WGSL layout for components would pad every `vec3` by a third and break the layout tier. It also has no notion of `entity`, `bool` as one byte, or enums.
- **As the material param vocabulary: borrowed, not imported.** A material's params are exactly a WGSL uniform struct, which is TypeGPU's model. bevy-ts's `bevy-ts/material` types mirror the `vec3f`-style names so authors who know TypeGPU or WGSL read them directly, and Rust computes the layout, as it does for components (bridge decision 3).
- **As the shader language: not now.** Its codegen works without a GPU device (`tgpu.resolve` is device-free), but JS-bodied functions need `unplugin-typegpu` at build time to embed their AST; bevy-ts compiles with oxc in the engine process and its loader rejects bare npm specifiers (bridge script-runtime), so adoption means porting the plugin to an oxc pass and vendoring a 0.x library that changes every few weeks into the built-in modules. Its resolved output is WGSL plus TypeGPU bind group layout objects that nothing maps to Bevy's `#{MATERIAL_BIND_GROUP}` conventions or PBR imports. WGSL strings with generated bindings give custom materials today with none of that.
- **Owning the GPU: never on native.** Bevy's wgpu device is the only one. On the web, both would sit on the browser's WebGPU, which is still not a reason to give TypeGPU the device.
- Revisit when TypeGPU reaches 1.0 and offers a codegen-only entry that emits WGSL functions for a host's bind group conventions; a `typegpu`-authored WGSL string can already be pasted into `fragment` today.

### 10. Script materials: one erased Rust material, runtime layouts
`bevy-ts-material` registers one Rust asset type, `ScriptMaterial`, implementing `ErasedRenderAsset` by hand, as `manual_material.rs` does: its `MaterialProperties` carry a bind group layout built per material kind from the TS schema, the kind's shader handles, and an `UnpreparedBindingResources` per instance (the uniform bytes and texture views). Rust lays params out with WGSL uniform rules and prepends the generated struct and bindings to the author's WGSL. Shader edits replace the shader asset in place; Bevy re-specializes.

- *Beat: one Rust `Material` type per material (`AsBindGroup` derive).* Every new material would be a Rust change and a project engine; the layout fn is static per type.
- *Beat: `ExtendedMaterial<StandardMaterial, _>` with a fixed extension of generic slots (N floats, M textures).* No layout logic, but authors index `params[3]` instead of naming fields, and the slot count is a cap someone will hit.
- *Beat: TS-authored render passes too.* Passes run in the render world on another thread with ordering rules (`chain_weak`) TS cannot see; settings components extracted by Rust cover the TS side.

### 11. Third-party plugins come only through the native crate
The prebuilt engine ships Bevy's default plugins, `bevy_remote` and bevy-ts's crates, and no third-party plugin. A project that wants avian, bevy_hanabi or any other plugin adds it in its native crate.

- *Beat: a curated set of plugins in the prebuilt engine.* Every included plugin pins its version for every user, grows the download for those who do not use it, and lags Bevy releases; it would also make bevy-ts responsible for those plugins' types in `bevy-ts/bevy`.
- *Beat: prebuilt flavours (`@bevy-ts/engine-physics-*`).* Combinations multiply per platform. Physics included: the operator ruled (2026-10-19) that the prebuilt engine ships none.

## Depends on bridge

- **N1. Public native layout registry.** bridge decision 4's registry SHALL be public so `NativeLayout` can register components, and the TS-system checks SHALL consult it for every Rust component, not only `Transform`.
- **N2. A sixth seam operation, call host function.** The backend trait SHALL let Rust register host functions JS can call synchronously during a system run or entry call, receiving views over JS memory and returning a view over a Rust buffer.
- **N3. Stage systems as a reusable unit.** The exclusive stage system SHALL be constructible for any schedule and ordering, so slots are bridge stages with a different placement, not a second runner.
- **N4. The engine entry is a library function.** `bevy-ts-cli`'s `main` SHALL be a thin call into `bevy_ts_cli::run(native: Option<fn(&mut NativeApp)>)`, which the generated project `main` also calls.
- **N5. `bevy-ts/hello` and `bevy-ts/schema` carry engine identity, exposed sets and native functions** (protocol minor bump).

## Data across the boundary

- **Layout-tier component:** identical to bridge's columns; the layout comes from `offset_of!` instead of bridge's C-rule computation, and the two agree for `repr(C)`.
- **Native function call:** JS passes `(fn id, input view: ptr, len, record count)`; Rust runs `f` with its `SystemState<P>` against `&World` (read-only, validated), writes results into a per-function output arena, and returns `(ptr, len, record count)`; JS views it with the output record's accessors. The arena is reset after the run, and the views are detached with the column views.
- **Script material instance:** uniform bytes in the kind's WGSL layout, encoded in Rust from a field map on mount and patch (TSX), or written by a TS system through a material component, decided by task 6.4.
- **Types:** `native.d.ts`, generated; never crosses at runtime.

## Risks / Trade-offs

- [The first `bevy-ts dev` in a native project compiles Bevy, minutes on a laptop] → Shared cache from CI; `bevy-ts dev` prints the hash it is building so the author knows a cache miss happened; dev profile with dependencies optimised once and reused.
- [A restart on every Rust edit loses world state] → Rust edits are rarer than TS edits; levels persist in `.bsn`; hot patching is the planned later change.
- [Every exposed set adds two exclusive systems, each a barrier for the parallel executor] → Sets are exposed by name only; empty slots are skipped by run condition. Whether a skipped exclusive system still costs a barrier is unverified (task 4.4 measures it); if it does, slots are created only for names the current generation uses, which needs one schedule rebuild when that set changes, never on ordinary reloads.
- [`bevy-ts types` must build the app without a GPU, and plugins may touch the device during build] → Types are read after `Plugin::build`, before `finish`, where Bevy's renderer creates its device; a plugin that needs a device in `build` is reported and its types are skipped (task 3.4 verifies with `bevy_rapier3d` and `bevy_egui`, the 0.20 plugins available today).
- [`ScriptMaterial` puts many layouts behind one `TypeId`-keyed bind group allocator] → `allocate_unprepared` takes the layout per call, so non-bindless allocation per kind should work; task 6.1 is a spike, and the fallback is one Rust type per kind from a fixed pool registered at startup.
- [A shared cache serves executables] → SHA-256 verified before execution; the cache location is the project's own choice; no default remote.
- [Exact version pins make bevy-ts upgrades a Cargo upgrade too] → The error names the versions; `bevy-ts upgrade` can rewrite the pin later.
- [Third-party plugins lag Bevy: avian and bevy_hanabi are still on Bevy 0.19 while bevy-ts is on 0.20] → A project can only use plugins that support bevy-ts's Bevy version, as any Bevy app; bevy-ts's own Bevy upgrades are announced with the plugin versions known to work, and `bevy-ts.json` cannot override the Bevy version. Physics today means `bevy_rapier3d` 0.37 or waiting for avian on 0.20.
- [Native functions run synchronously inside the JS stage, so a slow one stalls the frame] → They are Rust and batched; the per-system timing in `bevy-ts/systems` attributes time spent in native calls separately.

## Migration Plan

None: additive. Projects without a native crate are unchanged and keep the prebuilt engine. Protocol minor bump for N5.

## Operator rulings

- 2026-10-19: the prebuilt engine ships no physics and there are no prebuilt flavours. A project
  adds physics (bevy_rapier3d today, avian once it supports Bevy 0.20) through its native crate,
  built once per Rust change or fetched from the shared engine cache.

## Open Questions

- Whether native resources (single-row layout-tier views, `res: [NavSettings]`) belong here or with TS-defined resources in a later change.
- Whether material instances are assets addressed by key or components on the entity (task 6.4).
- Whether `bevy-ts build` packages per-platform game builds in this change or in a `ship` change.
