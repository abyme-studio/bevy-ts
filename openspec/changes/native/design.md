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
| Physics, solvers, collision | Broadphase trees, contact graphs and islands are pointer-heavy heap structures, not columns; solvers want SIMD and threads | A third-party plugin (avian, rapier) in the native crate | Layout-tier components it reads (velocities, forces); slots `systems(s).after(PhysicsSet.StepSimulation)`; exported params for raycasts and shape casts (decision 8) |
| Particles | Simulated on the GPU (bevy_hanabi) or in parallel CPU systems | Native crate | Spawner settings as layout-tier components; effect assets by path |
| Spatial queries at scale (neighbours, navmesh, pathfinding) | The index is a heap structure; per-entity queries from JS would cross per call | Native crate, as a resource | Exported param methods, with batched variants for bulk queries (decision 8) |
| Render-world work: render-graph systems, passes, compute, readback, custom draw | Runs in the render sub-app on the render thread, with no JS and no main-world access | Native crate | Settings as layout-tier components on cameras or entities, extracted by Rust; materials are the exception below |
| Custom materials | Not disqualified: a material is WGSL plus parameters, which is data | Prebuilt engine (the `script-materials` change) | `material()` declarations, TSX `<material>` |
| Parallel systems | JS is exclusive on the main thread (bridge decision 1) | Native crate | Ordering through slots; data through shared components |
| Third-party Bevy plugins | They are Rust crates, compiled in | `bevy-ts.json` (decision 11); the native crate only for the project's own Rust | Their reflected types (reflect tier), their sets if the crate exposes them, their components with `NativeLayout` if the crate author or a wrapper adds it |
| Components with non-POD data (`String`, `Vec`, `Handle<T>`, `Box<dyn T>`, maps) | No fixed layout, need drop and clone in Rust | Native crate, as Rust types | Reflect tier: typed values in TSX and the protocol, filters in queries; `Entity` refs are plain layout-tier fields |
| Platform integrations (Steamworks, console SDKs, IAP, native dialogs, haptics) | C SDKs, callbacks on foreign threads, platform build rules | Native crate | Exported param methods for requests, components or entries for results |
| Hot loops over huge counts that are plain arithmetic (boids, bobbing, damage over time) | Nothing: these stay TS | TS systems | — |

## Decisions

### 1. The project's Rust is one Bevy plugin, statically linked into a project engine
A project names one Cargo library and one plugin in it in `bevy-ts.json`:

```jsonc
"native": { "crate": "native", "plugin": "native::GamePlugin" }
```

The plugin is an ordinary Bevy `Plugin`. bevy-ts generates `.bevy-ts/engine/`, a binary crate whose `main` calls `bevy_ts_cli::run(plugins)` with the project's plugin, the adopted crates' plugins and the ports (decision 11). It builds that crate with `cargo`. The project engine is the prebuilt engine plus those plugins, nothing else: same CLI flags, same protocol, same TS loader. What TS needs from Rust is registered through an extension trait on `App`, Bevy's `register_type` idiom: `app.ts_param::<P>()` (decision 8) and `app.ts_set(name, schedule, set)` (decision 7).

- *Beat: a `bevy_ts::native!` entry macro with a `NativeApp` wrapper (the previous draft).* The wrapper restricted nothing it stated. "Exactly one entry" needed link-time tricks, where a plugin path named in config fails with an ordinary compile error. A third-party plugin and the project's own Rust are the same case.
- *Beat: dynamic libraries loaded at runtime (GDExtension, Unity native plugins, Bevy's removed `bevy_dynamic_plugin`).* Rust has no stable ABI. A dylib sharing Bevy types with the engine must be built by the same compiler against the same Bevy build with the same features, which is exactly a project-specific build anyway, plus unsoundness when anything drifts; Bevy removed its dynamic plugin loader in 0.15 for that reason (#14534), and godot-rust's own book advises turning GDExtension reload off. GDExtension avoids it by putting a C ABI over the whole engine, which for Bevy would mean wrapping every plugin's API in C.
- *Beat: a C ABI plugin surface (`abi_stable`, `stabby`).* It would let one prebuilt engine load plugins, but third-party Bevy plugins are written against Bevy's Rust API and would not cross it. It solves the toolchain problem only for code written specifically for bevy-ts.
- *Beat: letting the native crate own `main`.* Then every project carries engine startup code that must track bevy-ts releases. A generated `main` keeps the engine one thing.

### 2. Rebuild only when Rust inputs change, keyed by a content hash, with a shared directory cache
The engine hash covers the bevy-ts version, target, profile, engine features, `Cargo.lock`, the workspace's `Cargo.toml`s and the native package's files; never TS or assets. A hit in `.bevy-ts/engines/<hash>/` starts without `cargo`. A miss tries the shared cache, a directory (a network share, or a CI artefact a team unpacks), verifying a SHA-256, then builds. That is how "no Rust recompile for gameplay" holds for a team: gameplay authors fetch the engine their checkout needs, and only the people editing Rust compile. Before a build starts, `bevy-ts dev` prints what it will compile and why: the hash, the cache miss, and on a first build the size of Bevy, rusty_v8 and the plugins.

The SHA-256 next to a binary in the same cache proves integrity, not authenticity: whoever can write the cache can replace both. The cache location is the project's own choice and there is no default remote.

- *Beat: always `cargo build` and let Cargo's own freshness check decide.* Correct, but it requires a toolchain on every machine and pays Cargo's resolution on every launch.
- *Beat: committing engine binaries to the repository (or LFS).* It works with no service, but binaries per platform per change bloat history and drift from the source that built them; a hash-keyed cache cannot drift.
- *Beat: a bevy-ts hosted build service.* It is infrastructure and someone pays for it.
- *Later, not now:* an HTTP(S) URL template for the shared cache and a `bevy-ts engine publish` command. A directory covers a team, and CI can produce one.

### 3. Rust edits restart the engine; they never patch it
Under `bevy-ts dev`, a save in the native crate rebuilds in the background while the old engine runs. On success the new engine starts on the same port and the TS generation loads as a restart (startups re-run, TSX remounts). On failure the old engine keeps running. World state does not cross the restart; Rust edits are rarer and coarser than TS edits, and the studio reconnects through `bevy-ts/hello`.

- *Beat: `subsecond` hot patching (Bevy's `hotpatching` feature).* It keeps state across Rust edits, but it patches only the binary crate (the native crate is a library), cannot reload a system whose parameters changed, cannot change layouts or registrations, and needs `dx serve` in the loop. Room left: a later change can patch when the edit qualifies and restart otherwise.
- *Beat: carrying world state across the restart through reflection.* TS components would serialise, but Rust components without `Reflect` would not, and the result would be a partial world that looks whole. Levels in `.bsn` (tsx ruling) are the durable state.

### 4. Version lock-step: the native crate pins bevy-ts and Bevy exactly
`bevy-ts-native` is published to crates.io at the npm package's version, re-exports `bevy`, and the native crate depends on it with `=x.y.z`. bevy-ts reads `Cargo.lock` before building and refuses a mismatch. A minor bevy-ts release that moves Bevy forces the project's third-party plugins to move with it, as any Bevy app does.

- *Beat: a semver range.* Two bevy-ts patch releases can carry different engine internals that the native crate links against; with a generated `main` linking the same crates, only an exact match is known to work.

### 5. Rust components cross in two tiers, decided per type from reflection
Layouts come from reflection, not from `repr(C)` (operator ruling 2026-10-20, docs/research/dx.md). At startup the engine walks the type registry. For each registered component and resource with `ReflectComponent`, it takes a value (`ReflectDefault`, `ReflectFromWorld`, or the first live row) and reads every reflected field's offset as the field reference's address minus the value's address. A derived `Reflect` hands out references into the value itself.

The type is **layout tier** when it has at least one reflected leaf of a plain-data type bevy-ts knows: `f32`, `f64`, the integers, `bool`, `Entity`, glam's vectors and quaternions, and fieldless enums. Layout tier makes it a zero-copy column in TS systems, with change ticks, whatever its `repr`. A leaf of a type bevy-ts cannot read (reflection marks it opaque, such as `Duration`, or it is not plain data) is a **hole**. A hole is never exposed or written. The known leaves around it are still columns, and `q.tables` raw writes are disabled for that type. `Time` is the case that decides this: its `delta: Duration` is opaque (`impl_reflect_opaque!` in `bevy_reflect/src/impls/core/time.rs`), while its `delta_secs: f32` and `elapsed_secs: f32` are plain data, so `Res<Time>` reads `delta_secs` from Bevy's own resource. A type with no known leaves at all is **reflect tier**: typed in TS, usable in TSX elements, the protocol and plugins through bridge decision 14, and in `With`/`Without` filters, never as query data. A component that needs both is split into two components on one entity.

Guards on the layout tier:

- **Bounds:** every derived offset lies inside `size_of::<T>()`, leaves do not overlap, and each leaf's size matches its `TypeId`. Otherwise the type is reflect tier, with the reason in `bevy-ts types`.
- **Validity:**
  - `bool`, fieldless enums and `Entity` fields are accessor-only. Their setters write `0`/`1`, a byte from the variant table, or a validated id.
  - `q.tables` does not expose them raw.
  - A fieldless enum's variant bytes are read from this build at startup, never assumed.
- **Mutability:** a component declared `#[component(immutable)]` is never writable. Field writes run no hooks, as with Bevy's `Mut`.
- **Privacy:** reflected private fields are writable, as they already are to `bevy_remote`'s `world.mutate_components`. `#[reflect(ignore)]` fields are never touched.

`#[derive(NativeLayout)]` stays for components without `Reflect`. It fills the same registry from `core::mem::offset_of!`, which needs public fields but no `repr(C)`. Proven on `bevy_rapier3d` 0.37 (`bench/ports/`):

- `offset_of!` and reflection agree on every field of Rapier's `Velocity` and Bevy's `Transform`.
- Reflection also reaches glam's SIMD `Quat` and the private `LockedAxes.0`, which `offset_of!` refuses.
- 1,000 `Velocity` rows were written through an `f32` view and read back through Bevy's own `Query`.

- *Beat: requiring `#[repr(C)]` and a derive (the previous draft).* No third-party plugin's components qualify, so every adopted type needed a hand-written mirror plus systems that copy it in and out around each step.
- *Beat: generated `offset_of!` code per adopted type.* It needs public fields, cannot see inside glam's SIMD types, and needs codegen per type, where reflection needs none.
- *Beat: TS components with heap fields via a drop fn.* `new_with_layout` accepts a drop fn, but the data would be opaque bytes to TS anyway; the owner of heap data must be Rust.

### 6. TS types come from the engine's registry, generated, not hand-written
`bevy-ts types` builds the engine's `App` with all plugins added, stops before the renderer creates a device, walks `AppTypeRegistry` for components registered for TS, exposed sets and exported params, and writes `.bevy-ts/types/native.d.ts`. Layout-tier fields map to bridge field types; reflect-tier fields follow `registry.schema`'s JSON Schema. The prebuilt engine's types ship in the npm package as `bevy-ts/bevy`. Output is deterministic and diffed in CI, the way bridge's protocol types are.

- *Beat: hand-written `declare` blocks.* They drift on the first Rust edit.
- *Beat: a proc macro writing TS at compile time.* It sees one crate at a time, so it misses third-party plugins' reflected types and anything registered at runtime.
- *Beat: generating from the `bevy-ts/schema` protocol method of a running engine.* Same data, but needs a window and a running game; the registry walk can run in CI with no GPU.

### 7. TS orders against Rust through named sets, with slots made when first named
Rust exposes a system set under a name with `app.ts_set("rapier:step", FixedUpdate, PhysicsSet::StepSimulation)`. The first generation that orders a TS system against that name makes two exclusive stage systems around the set, a before slot and an after slot. That is one schedule edit at that reload. TS systems say `systems(s).after(systemSet("rapier:step"))` and run in the slot. Later reloads move systems between existing slots without touching the schedule. A slot no generation has named does not exist. Rust orders against TS through `BevyTsSet` and the slot sets.

A skipped exclusive system still costs the parallel executor a barrier. `MultiThreadedExecutor::can_run` refuses an exclusive system while `num_running_systems > 0` (`bevy_ecs/src/schedule/executor/multi_threaded.rs:533`), before its run condition is evaluated. So pre-built empty slots would add two drain points per exposed set to every schedule run. Rapier's three sets would add six.

- *Beat: two slots per exposed set built at app build, empty ones skipped by a run condition (the previous draft).* It would pay the barriers above on every frame for sets no TS system uses.
- *Beat: one Bevy system per TS system, inserted against the Rust set on each reload.* Precise, but it rebuilds schedules on every save, the cost bridge decision 1 refused.
- *Beat: only the four bridge stages.* A TS system reacting to physics results would run a frame late, or the author would have to move it to Rust.

### 8. TS calls Rust through Bevy system params with exported methods
Behaviour reaches TS as Bevy's own unit, a read-only `SystemParam`, and is used from TS as a Rust system uses it. A TS system takes the param in its signature (`rapier: ReadRapierContext`) and calls its methods (`rapier.castRay(...)`). The Rust side is one attribute on an `impl` block:

```rust
#[bevy_ts::export(ts = "ReadRapierContext", module = "bevy-ts/rapier")]
impl ReadRapierContext<'_, '_> {
    fn cast_ray(&self, origin: Vec3, dir: Vec3, max_toi: f32, solid: bool, f: RayFilter) -> Option<RayHit> { ... }
    fn cast_rays(&self, rays: &[Ray], solid: bool, f: RayFilter, out: &mut [RayHit]) -> u32 { ... }
}
// in the plugin: app.ts_param::<ReadRapierContext>();
```

The macro generates four things:

- a `SystemState<P>` kept in the backend's state;
- one host function per method, a fast op on V8;
- record codecs for the arguments;
- the method's TS declaration and the JS glue, in a built-in module the port registers (`app.ts_module("bevy-ts/rapier", ...)`).

The TS types therefore come from the Rust signatures. Signatures are restricted to scalars, `Vec2`/`Vec3`/`Quat`, `Entity`, `bool`, `NativeLayout` records, slices of them, `&mut` slices for output, and `Option` of these. A batched method is just another method on the same param: one call per batch, for the 1,000-ray case.

During a JS stage the backend holds the world for the run and fetches each param with `SystemState::get(&World)`. That access is read-only, so a method sees JS's writes made so far in the run, as an overlapping query does (ts-systems: overlapping queries need no ParamSet), and cannot move or free column memory. Methods cannot make structural changes; they return entity ids, and the TS system uses its `Commands`. Errors:

- a param that fails validation (`SystemParamValidationError`, such as Rapier's context not existing yet) throws a JS exception naming the param and method;
- a panic in a method is caught with `catch_unwind` and becomes an exception;
- a method cannot call back into JS.

- *Beat: named batched functions with fixed records, `NativeFn<"physics:raycast">` (the previous draft).* It is not Bevy's idiom, and a fixed `Ray` record cannot carry Rapier's `QueryFilter` (flags, groups, exclusions). The bench's own Rapier example already used the param form.
- *Beat: rejecting a function whose param reads a column the system writes (the previous draft).* It contradicts the ts-systems rule that overlapping access is sound inside the exclusive stage; read-only access is the whole bound.
- *Beat: Bevy's function reflection (`bevy_reflect::func`, `DynamicFunction`, `App::register_function`, feature `reflect_functions`).* Its arguments are boxed `PartialReflect` values, one allocation per argument per call, and a function cannot receive `&World` or a `SystemParam`. bevy_mod_scripting builds its own registry for the same reason.
- *Beat: one-shot systems (`World::run_system_with`).* They flush the world after the run (`system_registry.rs`), which would apply queued structural commands under JS's live column views.
- *Beat: requests as commands, results next frame.* A frame of latency for every raycast changes gameplay.
- *Beat: giving methods `&mut World`.* JS holds live column views during the run; a mutating method could move or free the memory under them.

### 11. Third-party plugins are adopted by one line of config
The prebuilt engine ships Bevy's default plugins, `bevy_remote` and bevy-ts's crates, and no third-party plugin. A project adopts a plugin in `bevy-ts.json`, never in code (operator ruling 2026-10-20). The project's own crate (decision 1) is named the same way:

```jsonc
{ "native": {
    "crate": "native", "plugin": "native::GamePlugin",                       // the project's own Rust
    "ports": ["rapier3d"],                                                   // curated
    "crates": { "bevy_kira_audio": { "version": "0.27", "plugin": "bevy_kira_audio::AudioPlugin" } }  // uncurated
} }
```

bevy-ts generates `.bevy-ts/engine/` (decision 1) with the dependencies and the plugin lines, and builds the project engine (decision 2). The registry walk then types every reflected component and resource the plugin registered, as layout or reflect tier per decision 5. That is the data layer, with no code per type.

What reflection cannot reach is the behaviour layer, and comes from a curated port or the project's own native crate:

- behaviour, such as raycasts (exported system params, decision 8);
- components without `Reflect`, such as Rapier's `Collider`, which come as TSX constructors;
- messages that are enums with data, such as Rapier's `CollisionEvent`, which a port flattens to a layout record;
- system sets;
- plugin construction that needs code, such as `RapierPhysicsPlugin::<NoUserData>::default()`.

A curated port is a crate `bevy-ts-<name>` whose Bevy plugin adds the third-party plugin, exports its params (decision 8), exposes its sets (decision 7) and registers its built-in TS module `bevy-ts/<name>` (types, JS glue, TSX constructors, flattened messages). The Rapier port constructs `RapierPhysicsPlugin::<NoUserData>::default().in_fixed_schedule()`, because Rapier's default schedule is `PostUpdate` (`plugin/plugin.rs`) and TS physics gameplay belongs in `FixedUpdate`. bevy-ts publishes these ports, a few hundred lines each:

- `rapier3d` now;
- `leafwing-input-manager`, whose `Actionlike` is generic over the game's own Rust enum, so the port ships a dynamic `TsAction(u16)` whose names come from TS;
- `avian3d` and `bevy_hanabi` once they support Bevy 0.20.

A native crate is needed only for the project's own Rust.

- *Beat: plugins only through the native crate (the previous draft).* Every adoption was Rust code and glue.

- *Beat: a curated set of plugins in the prebuilt engine.* Every included plugin pins its version for every user, grows the download for those who do not use it, and lags Bevy releases; it would also make bevy-ts responsible for those plugins' types in `bevy-ts/bevy`.
- *Beat: prebuilt flavours (`@bevy-ts/engine-physics-*`).* Combinations multiply per platform. Physics included: the operator ruled (2026-10-19) that the prebuilt engine ships none.

## Depends on bridge

- **N1. Native layout registry filled from reflection.** bridge decision 4's registry SHALL be filled at startup from the type registry by decision 5's rule, SHALL be public so `NativeLayout` can add unreflected components, and the TS-system checks SHALL consult it for every Rust component; `Transform` becomes the first case of the rule, not a special registration.
- **N2. A sixth seam operation, call host function.** The backend trait SHALL let Rust register host functions, one per exported method, that JS can call synchronously during a system run or entry call. They receive scalars and views over JS memory and return scalars or a view over a Rust buffer. On V8 they are fast ops (`#[op2(fast)]`), and on QuickJS-ng and the web they are host functions and wasm imports. During a run the backend makes the world reachable read-only to them.
- **N3. Stage systems as a reusable unit.** The exclusive stage system SHALL be constructible for any schedule and ordering, so slots are bridge stages with a different placement, not a second runner.
- **N4. The engine entry is a library function.** `bevy-ts-cli`'s `main` SHALL be a thin call into `bevy_ts_cli::run(extra: impl Plugin)`, which the generated project `main` also calls with the project's, adopted crates' and ports' plugins.
- **N5. `bevy-ts/hello` and `bevy-ts/schema` carry engine identity, exposed sets and exported params** (protocol minor bump).

## Data across the boundary

- **Layout-tier component:** identical to bridge's columns; the layout comes from reflection (or `offset_of!` for `NativeLayout` derives) instead of bridge's C-rule computation.
- **Exported method call:** one host call per method call. Scalars and vectors pass as numbers or a reused scratch view; records and slices pass as views over JS memory. Rust gets the param from its `SystemState<P>` against the world (read-only, validated), runs the method and writes record results into the param's output arena. JS views those through generated accessors. The arena is reset after the run, and its views are detached with the column views.
- **Types:** `native.d.ts`, generated; never crosses at runtime.

## Risks / Trade-offs

- [The first `bevy-ts dev` in a native project compiles Bevy, rusty_v8 and the plugins, minutes on a laptop] → `bevy-ts dev` prints the plan (hash, cache miss, what will compile) before starting; shared directory cache from CI; dev profile with dependencies optimised once and reused.
- [A restart on every Rust edit loses world state] → Rust edits are rarer than TS edits; levels persist in `.bsn`; hot patching is the planned later change.
- [Each slot is an exclusive system and a barrier for the parallel executor, skipped or not (verified in source, decision 7)] → Slots exist only for sets a generation has named, made by one schedule edit at the reload that first names a set; task 4.4 measures the barrier's cost.
- [`bevy-ts types` must build the app without a GPU, and plugins may touch the device during build] → Types are read after `Plugin::build`, before `finish`, where Bevy's renderer creates its device; a plugin that needs a device in `build` is reported and its types are skipped (task 3.4 verifies with `bevy_rapier3d` and `bevy_egui`, the 0.20 plugins available today).
- [A shared cache serves executables] → SHA-256 verified before execution, which proves integrity, not authenticity (whoever can write the cache can replace both); the cache location is the project's own choice; no default remote.
- [Exact version pins make bevy-ts upgrades a Cargo upgrade too] → The error names the versions; `bevy-ts upgrade` can rewrite the pin later.
- [Third-party plugins lag Bevy: avian and bevy_hanabi are still on Bevy 0.19 while bevy-ts is on 0.20] → A project can only use plugins that support bevy-ts's Bevy version, as any Bevy app; bevy-ts's own Bevy upgrades are announced with the plugin versions known to work, and `bevy-ts.json` cannot override the Bevy version. Physics today means `bevy_rapier3d` 0.37 or waiting for avian on 0.20.
- [Exported methods run synchronously inside the JS stage, so a slow one stalls the frame] → They are Rust, with batched variants for bulk work; the per-system timing in `bevy-ts/systems` attributes time spent in method calls separately.
- [Every port ships four artefacts per method (op, codec, types, JS glue)] → `#[bevy_ts::export]` generates all four from the Rust signature.

## Migration Plan

None: additive. Projects without a native crate are unchanged and keep the prebuilt engine. Protocol minor bump for N5.

## Operator rulings

- 2026-10-20: applied the native review (docs/research/dx.md). TS calls Rust through exported
  system params (`#[bevy_ts::export]`), not named functions. The project's Rust is a plain Bevy
  plugin named in config, with no `native!` macro. Unreadable leaves are holes, so `Res<Time>` works.
  The aliasing rejection is dropped. Slots exist only for named sets. The Rapier port runs in
  `FixedUpdate`. Only the directory shared cache ships now. Script materials are their own change.

- 2026-10-20: layouts come from reflection, not `repr(C)`: any reflected component whose leaves are
  plain data is a zero-copy column. Plugins are adopted by one line in `bevy-ts.json`
  (`native.ports`, `native.crates`); bevy-ts curates ports for what reflection cannot reach, Rapier
  first. A native crate is only for the project's own Rust (docs/research/dx.md, question 2).

- 2026-10-19: the prebuilt engine ships no physics and there are no prebuilt flavours. A project
  adds physics (bevy_rapier3d today, avian once it supports Bevy 0.20) through its native crate,
  built once per Rust change or fetched from the shared engine cache.

## Open Questions

- Whether `bevy-ts build` packages per-platform game builds in this change or in a `ship` change.
