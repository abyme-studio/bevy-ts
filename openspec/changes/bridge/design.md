## Context

bevy-ts is empty: a README and a runtime study (`docs/research/js-runtime.md`). Rulings of 2026-10-19 fix the frame: Bevy 0.20, Rust owns the one ECS world, V8 through `deno_core` on desktop behind an engine-neutral bridge, TS components as real ECS components with native layouts, TS systems over queries with no crossing per component, hot reload with no Rust rebuild, and the Bevy Remote Protocol as the only coupling to abyme's Bevy studio. TSX scenes and UI come in a later change.

Prior art read for this design (sources pulled from crates.io on 2026-10-19):

- **Bevy 0.20 `bevy_ecs`.** `ComponentDescriptor::new_with_layout(name, storage, layout, drop, mutable, summary_tick, clone_behavior, relationship_accessor)` plus `World::register_component_with_descriptor` give real components from a runtime layout. `QueryBuilder::ref_id`/`mut_id` build queries over `ComponentId`s, `QueryState::matched_tables` and `Table::get_column` reach column memory, and `EntityWorldMut::insert_by_id` inserts raw bytes. 0.20's new contiguous iteration (`Query::contiguous_iter_mut`, `ContiguousMut`) is the in-engine precedent for handing out whole column slices; it only works for typed `T`, so bevy-ts reuses its rules, not its API: a mutable chunk marks every row changed, and summary ticks let readers skip unchanged tables. `EntityAllocator::alloc` hands out real entity ids without `&mut World`. There is no way to unregister a component.
- **Bevy 0.20 `bevy_remote`.** `RemotePlugin::with_method_main` / `with_watching_method_main` register custom methods; the HTTP transport streams watching methods as server-sent events on 127.0.0.1:15702; handlers run in `RemoteLast` with `&mut World`. Every stock component method resolves names through `TypeRegistry::get_with_type_path` and `ReflectComponent`, so a dynamic component with no Rust type is invisible to `world.get_components`, `world.query`, `world.mutate_components` and `registry.schema`. That single fact shapes the protocol half. `rpc.discover` emits OpenRPC 1.3.2 but lists method names only, with no param schemas. `registry.schema`'s JSON Schema shape (`JsonSchemaBevyType`) is reused for TS components.
- **bevy-react 0.8.1** (`bevy_react_core`, `deno_core` 0.404, Bevy 0.20). Reused: the vendor/app split, keeping the isolate alive across reloads, and falling back to the last good code when a reload throws (`js_thread.rs`); deterministic codegen checked with `git diff --exit-code` (`ts_codegen.rs`). Not reused: its dedicated JS thread with a current-thread tokio runtime, which costs a thread hop per round trip (about 1 to 6 ms measured by its author). That is right for a reconciler, wrong for per-frame systems. Also not reused: its external `esbuild --watch` plus bundle mtime polling, and its JSON op batches.
- **bevy_mod_scripting 0.23.** Registers script components with `new_with_layout`, but every one shares `DynamicComponent`'s layout (a boxed `ScriptValue`), so the data is opaque to Rust and to queries over fields; access goes through reflection per field. Its `ScriptSystemBuilder` adds script systems to schedules at runtime by removing and reinserting the schedule. bevy-ts takes the registration call and rejects the boxed-value model.
- **bevy_brp_extras.** A third-party BRP extension namespacing its methods `brp_extras/<name>`; bevy-ts follows the same `<crate>/<name>` convention.

## Goals / Non-Goals

**Goals:**
- TS declares components with native layouts and systems over them; JS touches columns directly, Rust applies structural changes.
- One call into JS per system run, whatever the entity count.
- `bevy-ts dev src/game.ts` reloads TS in about a frame or two with explicit rules for what survives.
- A seam a QuickJS-ng or browser backend can implement without touching the core.
- A versioned protocol file abyme's studio can generate a client from, covering reload, schemas (TS included), structural change events and field edits.

**Non-Goals:**
- TSX, the reconciler, BSN templates, UI (the `tsx` change). Room is left: the plugin API (decision 13), TSX compilation, `spawnNative` recipes and the entity stream are what a reconciler needs.
- TS-defined resources and events, asset handles in TS components, strings in components.
- QuickJS-ng and web backends (only the seam and a fake backend).
- Parallel JS systems, or JS running beside Rust systems.
- Writing inspector edits back into TS source, viewport frame streaming, an asset list over the protocol.

## Decisions

### 1. JS runs on the main thread, inside one exclusive system per stage
The `JsRuntime` is non-send data on the main thread. Each stage (`PreUpdate`, `Update`, `PostUpdate`, `FixedUpdate`) gets one exclusive system in `BevyTsSet::<Stage>` that runs that stage's TS systems in order. A reload swaps a list in a resource; the Bevy schedule never changes.

- *Beat: a dedicated JS thread (bevy-react).* Systems need the world synchronously each frame, so the main thread would block on every hop anyway, and the hop costs milliseconds.
- *Beat: one real Bevy system per TS system with declared access.* It would let non-conflicting Rust systems run beside JS, but the isolate pins them all to one thread regardless, and every reload would rebuild schedules (bevy_mod_scripting's remove-and-reinsert dance). It is the later optimisation: an exclusive stage can become a non-exclusive one declaring the union of its systems' access without changing the TS API.

### 2. Columns are zero-copy, field writes are in place, structure goes through a command buffer
"Mutations through a command buffer" is read as structural mutations: spawn, despawn, insert, remove, reparent. Field writes go straight into the column, and their change ticks with them (decision 6). Routing field writes through a buffer too would handle every written value twice and defeat the zero-copy columns the ruling asks for; measured at 2.5× the write loop at 1M rows before Rust applies anything (`docs/research/change-detection.md`).

- *Beat: copy columns in and out (Node's sandbox build, any engine with the V8 sandbox on).* That is two copies per column per run.
- *Beat: per-entity accessor calls into Rust (bevy_mod_scripting).* That is one crossing per field access.
- *Beat: field writes as commands.* See decision 6.

### 3. Rust computes layouts with C rules; JS only reads offsets
TS declares field types; Rust lays out the struct (C rules, see the ts-components spec), registers it, and returns offsets with the declarations. The same rules make every field offset a multiple of its own size, so one `Float32Array`, `Int32Array` or `Uint8Array` view per element width over the same buffer indexes every field exactly.

- *Beat: SoA, one Bevy component per field.* That would mean more columns but plain arrays. It would break "a TS component is one real component": inspector, `Changed`, insert and remove would all work per field, not per component.
- *Beat: JS computing layouts.* That would put two implementations in sync forever, and the Rust one is authoritative anyway.

### 4. Rust components cross only with an `offset_of!` layout
`Transform`'s Rust layout is not `repr(C)` and glam's `Quat` may be 16-aligned, so bevy-ts never guesses. A `NativeLayout` registry, filled with `core::mem::offset_of!` and `size_of`/`align_of`, maps fields for the Rust components it supports. This change supports only `Transform`.

- *Beat: reflection access for Rust components.* That is per-field crossings again.

### 5. Two iteration surfaces over the same views
`q.each((e, mover, t) => ...)` hands reused flyweight accessors whose getters index the typed arrays, readable and fast once V8 inlines them; their setters also store the row's change tick (decision 6). `q.tables` exposes the raw arrays and offsets for hand-tuned loops, as `table.write(T)` (whole table marked changed) or `table.split(T)` (arrays plus `ticks`, caller marks). Both read the same memory.

- *Beat: raw arrays only.* Too hostile for game code.
- *Beat: fresh objects per row.* That means allocation per entity per frame and GC pauses.

### 6. Per-row change ticks, written by JS into Bevy's own tick column
Bevy 0.20 keeps a column's changed ticks as a contiguous `u32` array parallel to its data (`Column::changed_ticks`, reachable through `Table::get_changed_ticks_slice_for`). For each `write` column the backend hands JS that array as a second zero-copy view, a `Uint32Array` beside the data, and the run's tick. A generated accessor setter stores the tick on the row it writes (`ticks[row] = tick`), so a row is changed when, and only when, a system wrote it, in the sense of Bevy's `Mut`: any write marks, a write of the same value included. After the run Rust detaches both views and sets the column's summary tick. Nothing is scanned, copied or remapped: the tick lives in the row and moves with it, and structural changes are deferred past the run anyway.

`q.tables` follows `ContiguousMut`'s two surfaces: `table.write(T)` returns the raw arrays and marks the whole table changed when the run ends (the `DerefMut` rule), and `table.split(T)` returns the arrays plus `ticks` and leaves marking to the caller.

Each TS system run takes its own tick from `World::increment_change_tick`, so a later TS-side `changed` read can be per system.

Settled by measurement (`docs/research/change-detection.md`, 2026-10-19): on V8 the tick store costs nothing measurable when few rows are written and about a fifth of the write loop when every row of 1M is written; without a JIT the loop is bound by property dispatch, and the store costs 10 to 26% on class accessors and nothing on the closure-scoped accessors the QuickJS-ng backend will generate (each backend's runtime picks its accessor form; the setter stores the tick in all of them). `Changed<T>` in 0.20 reads every row's tick and never the summary tick, so a coarse mark is paid by every consumer on every row of every written table, Bevy's own transform propagation and render extraction included.

- *Beat: marking every row of each `write` column after the run (0.20's `ContiguousMut` rule, the previous draft).* Its fill is cheap, but it makes `Changed<T>`, `bevy-ts/get_components+watch` and TSX's `useComponent` fire for every row of a table any system touched, 1M rows a frame when 3k moved. Unity DOTS lives with coarse detection only because its grain is a 16 KB chunk; a Bevy table is unbounded. Kept only as the `table.write(T)` rule, where Bevy itself applies it.
- *Beat: a separate dirty bitset turned into ticks by Rust.* Same JS cost as the tick store, plus a scan per column per run, plus a remap if rows ever moved. The tick array is the bitset.
- *Beat: a shadow copy diffed after the run.* One copy and one compare of every write column per run, 1 to 5 ms per 1M rows per column in Rust, whether anything changed or not. It would be the only path that marks only real changes; an author who wants that compares before writing, as Rust code uses `set_if_neq`.
- *Beat: field writes through the command buffer.* 2.5× the write loop at 1M rows before Rust applies a single op, every value handled twice, and a system no longer sees its own writes in the same run.
- *Beat: write intent per field, or per-page dirty bits.* Neither changes what Bevy records, which is one tick per row per component.

### 7. Commands are a binary arena, applied after each system
JS encodes ops (opcode, entity bits, component id, native-layout bytes) into a Rust-owned `Uint8Array`; Rust applies them with `insert_by_id`/`spawn_empty_at` right after the system returns, so the next system sees them, as if each TS system were followed by `ApplyDeferred`. `spawn` returns ids from a pool pre-filled with `EntityAllocator::alloc`; unused ids are freed after the stage. Overflow is held in JS and copied after the run, so Rust never allocates mid-run. That matters on the web, where growing wasm memory detaches every live view.

- *Beat: JSON op batches (bevy-react).* That pays serialisation per op, and component values would need decoding.
- *Beat: applying at the end of the stage.* That surprises authors, and views are rebuilt per system anyway.

### 8. Host-side TS through oxc inside the `bevy-ts` binary
`bevy-ts dev` is the engine binary itself. A background thread watches files with `notify`, strips types and emits source maps with `oxc_transformer`, and hands the engine a compiled module set. A custom `deno_core::ModuleLoader` serves them.

- *Beat: an external bundler (esbuild/Vite, bevy-react's way).* That would add a Node dependency and a second process to `bevy-ts dev`, and bundling loses per-module reload granularity later.
- *Beat: `deno_ast` (swc).* It works equally well; oxc is faster and is what the research recommended.

### 9. Soft reload in the same isolate, versioned module specifiers
Each generation loads as side modules under specifiers tagged with the generation (`file:///…/game.ts?gen=7`) into the existing isolate. The previous generation's systems are dropped from the registry and its module records stay in `deno_core`'s module map, a small dev-only leak. `restart: true`, or a heap above a threshold, builds a fresh isolate.

- *Beat: a fresh isolate per reload.* That is cleaner and leak-free, but every reload drops the CDP session, so the debugger detaches on every save. It also pays isolate startup on every save.
- *Beat: `Debugger.setScriptSource` live edit (Deno's `--watch-hmr`).* It keeps closures, but fails on many edits and cannot re-run declarations. The research already ruled it a bonus, not a requirement.

### 10. Layout change makes a new `ComponentId` and migrates by field name
Bevy cannot unregister a component, and an existing column's layout cannot change. So a changed layout registers a fresh id (Bevy-side name `game::Mover`, made unique internally by generation), copies values field by field, and removes the old component; the old id is retired. An undeclared component keeps its data so commenting a declaration out and back is lossless.

- *Beat: refuse layout changes until restart.* Adding a field is the most common edit.
- *Beat: reallocate in place.* It is not possible with Bevy's tables.

### 11. Protocol: `bevy-ts/` methods next to stock BRP, TS components served in stock shapes
Stock methods cannot see TS components (decision context above), so bevy-ts adds `get_components`, `get_components+watch`, `mutate_components` and `schema` that take stock params and return stock shapes for both kinds. The studio needs one code path per concern. Schemas reuse `registry.schema`'s JSON Schema with an `x-bevy-ts` extension. Structural changes stream from `bevy-ts/entities+watch`, fed by lifecycle observers (`Add`/`Remove`/`Despawn` on watched component ids, `ChildOf`, `Name`) registered only while a watcher is open and coalesced per frame.

- *Beat: overriding stock method names.* That silently changes Bevy's documented behaviour for any other BRP client.
- *Beat: registering a fake reflected type per TS component.* `TypeRegistration` needs a Rust `TypeId`, and a dynamic component has none.
- *Beat: polling `world.query` for the scene tree.* That costs a full scan per poll and still misses TS components.

### 12. The contract is an OpenRPC file, with types generated on both sides
`protocol/bevy-ts.openrpc.json` (OpenRPC 1.3.2, matching `rpc.discover`) holds methods, params and results as JSON Schema. Rust types come from `typify`; TS types come from `json-schema-to-typescript`, exported as `bevy-ts/protocol`. Both are checked in and verified by regenerate-and-diff in CI (bevy-react's codegen guarantee). The studio generates from the same file. Feature crates add their methods to this same file, under the same naming and semver rules, each tagged with its owning crate (`x-bevy-ts-crate`); there is one contract, not one per crate.

- *Beat: Rust types as the source, exported with `schemars`.* The studio's side would then depend on bevy-ts's Rust build, and the schema would be an artefact nobody reviews.
- *Beat: plain JSON Schema without OpenRPC.* That loses the method list, and BRP already speaks OpenRPC.

### 13. Feature crates extend the bridge through a Rust plugin API, not through the core
The TSX crate, and any later feature crate, needs hooks the core would otherwise have to grow per feature. `bevy-ts` exposes six, all engine-neutral (plugin-api spec):

- **Manifest sections.** A plugin registers a named section that JS fills while modules evaluate. The section is delivered to the plugin when the generation applies, and the plugin may reject it, which fails the generation.
- **Host-invoked entries.** These are the seam's fifth operation: a plugin calls a named JS entry with bytes and gets bytes plus a command buffer back. The plugin decides when to call (for example, only when a change tick moved), so an idle feature costs no JS call.
- **Plugin command kinds.** These are opcodes with Rust decoders that share the arena and its ordering with the built-in commands.
- **Component lookup and encoding.** A plugin can look up a component's id and layout by name, and encode a field map into native bytes.
- **Reflected component access.** See decision 14.
- **Restart notification.**

- *Beat: feature crates calling `JsRuntime` directly.* That breaks engine neutrality and the threading rules.
- *Beat: putting TSX inside the core.* That couples every backend and the core's release cycle to the reconciler.
- *Beat: plugins running their own per-frame JS stage.* It would always cross, even when nothing changed. A host-invoked entry crosses only when the plugin asks.

### 14. Rust components without a native layout are reachable from plugins by reflection, not from TS systems
`Text` and `Node` hold strings, vectors and enums with no fixed layout, so they cannot be columns. Plugins get two things for any component: change ticks by `ComponentId` (cheap, no reflection), and JSON read/write through `ReflectComponent` by type path. A feature like TSX's `useComponent` checks ticks every frame in Rust and pays reflection plus JSON only on frames where the value changed, carrying the result through an entry call. TS systems still reject these components in `read`/`write`.

- *Beat: native layouts for `Text`/`Node`.* They are not POD; `offset_of!` cannot describe a `String`.
- *Beat: deferring reflected reads to a later change.* That would leave the TSX HUD unable to observe UI components, and the cost is small because the stock `bevy_remote` code already does exactly this per request.

## Threading model

| Thread | Runs | Talks to the world |
|---|---|---|
| Main (winit, main app schedules) | Rust systems through Bevy's executor; the bevy-ts stage systems (exclusive, `spawn_on_scope`, so on this thread); JS execution, including plugins' entry calls, which are made only from exclusive systems; the CDP message pump; `RemoteLast` protocol handlers; applying generations in `First` | Directly, `&mut World` inside exclusive systems |
| Bevy compute task pool | Every non-exclusive Rust system | Through Bevy's scheduler, never during a JS stage |
| Render thread (pipelined rendering) | Bevy's render app | Extract only; unchanged by bevy-ts |
| `bevy-ts-dev` watcher (dev only) | File watching, oxc compile, source maps | None; sends compiled generations over a channel |
| `bevy_remote` HTTP (async task) | Parses requests, streams SSE | None; queues requests for `RemoteLast` |
| V8 inspector server (dev only) | CDP websocket | None; queues messages the main thread pumps |

A frame: `First` applies a waiting generation, then pumps the backend (timers, inspector). `PreUpdate`, `Update` and `PostUpdate` each run their Rust systems in parallel, and the bevy-ts stage exclusively inside its set. Each TS system takes a change tick, builds views (data and ticks), makes one call into JS, detaches the views, sets summary ticks and applies commands. JS writes ticks only inside that exclusive window, so no Rust system reads a column while its ticks move. `FixedUpdate` does the same per fixed step. `RemoteLast` answers protocol requests and flushes watch streams. A debugger pause blocks inside the JS call, so the whole frame waits.

## Data layout across the boundary

- **Declarations, once per generation:** JS hands Rust a JSON manifest (components with fields, types, defaults and hints; systems with stage, access and order; startups; entry names; plugin sections as opaque bytes, routed to their plugins). Rust answers with ids and layouts (size, align, field offsets). JSON is fine here; it never runs per frame.
- **Per system run, Rust to JS:** a frame descriptor in a reused `Float64Array`/`Uint32Array` pair: `dt`, `elapsed`, the run's change tick (from `World::increment_change_tick`), and for each matched non-empty table its row count. For each table × component, one external `ArrayBuffer` over `[column_ptr, column_ptr + len × size)`, with lazily created typed views per element width. For each table × `write` component, a second external `ArrayBuffer` over the column's changed ticks (`len × 4` bytes from `Table::get_changed_ticks_slice_for`), viewed as a `Uint32Array`. The entity column is copied into a reused `Uint32Array` as (index, generation) pairs. The command arena is a `Uint8Array` over Rust memory with a write cursor.
- **Per entry call:** a payload `Uint8Array` in, bytes and the command arena back. The format is the plugin's own.
- **Per system run, JS to Rust:** nothing but the arena's used length and the overflow, if any. Field writes are already in place.
- **After the run:** detach every external buffer, set the summary tick of every `write` column in every matched table (a summary tick set when nothing changed is safe under Bevy's contract; a missed one is not), fill the ticks of tables taken through `table.write(T)`, clear the cursor.
- **Value encoding for commands and protocol edits:** native layout bytes, little-endian (every target bevy-ts ships is little-endian); `entity` fields are `Entity::to_bits`, validated on read.

V8 sandbox caveat: external backing stores over arbitrary pointers need the sandbox off. If a future rusty_v8 forces it on, or a platform requires it, the views become copies into sandbox memory before and after each run, behind the same backend trait. That costs speed, not API. The web backend gets zero-copy for free (wasm memory is an `ArrayBuffer`), but its views must be rebuilt after any `memory.grow`, which per-run views already do.

## Hot-reload state rules

1. ECS state survives: entities, every component value, resources, parent links.
2. JS module state dies: module-level variables, closures, timers. State that must live goes in components.
3. Systems are replaced wholesale by the new generation; ordering is recomputed.
4. Startups run once per name per world; `restart: true` resets that, despawns TS-spawned entities and notifies plugins before applying.
5. Same name and layout means same `ComponentId`, untouched.
6. Changed layout means a new id with migration by field name: matching name and type copies, new fields take their default, removed or retyped fields are reported.
7. An undeclared component stays with its data, hidden from systems, shown as undeclared in schemas.
8. A failed generation (syntax, evaluation, declaration, migration) is never partly applied; the old one keeps running.
9. A generation applies whole at the start of a frame; no frame mixes two.
10. Inspector edits are ECS state, so they survive reloads and are lost on process exit.

## First-milestone demo

`examples/cube/src/game.ts`: a `game::Mover { speed, amplitude }` component, a startup `scene` that `spawnNative`s a camera, a light and an orange cuboid and inserts `Mover`, and a system `bob` in `update` that sets `translation.y = sin(elapsed × speed) × amplitude`.

1. `bevy-ts dev examples/cube/src/game.ts` opens a window with the cube bobbing.
2. Change `bob` to also move `x`, then save: within 200 ms the motion changes, the cube does not jump, and there is no second cube.
3. Add `phase: f32` to `Mover`, use it in `bob`, then save: the terminal reports `game::Mover` migrated on 1 entity, and `speed` and `amplitude` are kept.
4. `curl` `bevy-ts/mutate_components` with `.amplitude = 4`: the bob widens; `bevy-ts/get_components+watch` streams the new value.
5. Save a syntax error: the terminal shows file and line, the cube keeps bobbing, and `bevy-ts/reload+watch` reports `failed`.
6. Attach VS Code over `--inspect`, break in `bob`: the game freezes on the TS line; save a change and the breakpoint still hits.

## Risks / Trade-offs

- [A row marks changed on a write of the same value, as Bevy's `Mut` does] → Authors compare before writing when it matters, as Rust code uses `set_if_neq`; the comparing setter costs 2× the plain one on V8 and is not the default.
- [JS cannot write `changed_by` locations under Bevy's `track_location` feature] → Dev only, off by default. When it is on, the post-run step fills `changed_by` for rows whose tick equals the run's tick with a location naming the TS system.
- [JS can write into a `read` column: typed arrays cannot be made read-only] → Dev builds hash read columns around each run and warn; a serial hash costs ~8 ms per 1M rows per column (docs/research/change-detection.md), so it uses a SIMD hash or samples pages. Release builds trust the declaration, which only affects change detection, since the stage is exclusive and no other thread is reading.
- [A system that throws after writing fields leaves partial writes] → Commands from that run are discarded; field writes cannot be rolled back. Documented.
- [Module records leak across soft reloads] → They are dev only; an automatic hard reload triggers above a heap threshold.
- [Breakpoints may not rebind across generation-tagged specifiers in every debugger] → Source maps point at the stable TS path. Verify with VS Code and Chrome in task 7; fall back to stable specifiers plus a fresh isolate if they fail.
- [The JS stage blocks all Rust parallelism in its schedule] → Most game logic is cheap. The union-access non-exclusive stage is the planned fix, with no TS API change.
- [Retired `ComponentId`s accumulate over a long dev session] → Each costs a `ComponentInfo` entry and nothing per frame.
- [V8 binary size (about 38 MB) and the macOS JIT entitlement] → Accepted by the runtime ruling.

## Migration Plan

None: a greenfield crate. Protocol 1.0.0 ships with this change; the studio pins the major.

## Operator rulings

- 2026-10-19: the game draws inside abyme's window from the start, in a native hole, as abyme's
  browser view already sits behind its page. The engine renders to an offscreen target shared
  across processes as a GPU surface (IOSurface on macOS) instead of a window of its own; abyme's
  app draws that surface in the hole and forwards input over the protocol. macOS and this Mac
  first; streamed frames for remote devices come later. The edit viewport is the engine itself in
  edit mode (paused simulation, editor camera, engine-drawn gizmos). It is the `viewport` change,
  with its abyme half in abyme's Bevy app; this change keeps a windowed mode for `bevy-ts dev`
  outside abyme.

## Open Questions

- Viewport and asset viewer: this change specs no viewport surface, picking/selection sync or asset listing. They are the `viewport` change (operator ruling below) and protocol 1.1.
- Whether `bevy-ts/pause` should also offer single-frame stepping.
- Packaging: whether `bevy-ts` ships as one prebuilt binary per platform through npm (`npx bevy-ts dev`) or through `cargo install`. That decides how a user with no Rust toolchain gets it, and is left to the CLI task.
