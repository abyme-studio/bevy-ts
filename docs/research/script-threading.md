# TS beyond one main-thread isolate

Recommendation: lanes. A lane is one V8 isolate on a thread of its own that loads the whole
generation. Every TS system becomes a real Bevy system with its exact access, built at load, so
Bevy's parallel executor runs two TS systems at once when their access does not conflict and they
sit in different lanes. A module picks its lane; the default lane is `main`, which is today's
behaviour. The same lane mechanism hosts a render lane (Bevy's render thread hands it the frame's
`wgpu::CommandEncoder`), an asset lane (TS asset loaders on Bevy's asset pool) and a kernel pool
(`par` over rows). GPU work in TS is our own WebGPU-shaped ops over Bevy's device, sized to what
TypeGPU's `initFromDevice` needs; not deno_webgpu.

This is a proposal. The architecture and any new dependency need the operator's go.

Researched and measured 2026-10-23 on this Mac (Apple M5 Max, macOS 25.4), deno_core 0.412 (v8
150.4), Bevy 0.20, wgpu 30.0.1. Bench in `bench/threading/js/workers.js` (Deno 2.9.7, V8 15.0)
and `bench/threading/rust/` (deno_core directly), raw output in `bench/threading/results/`.

## What decides it

- **An isolate cannot move between threads here.** `JsRuntime` is `!Send`, and the `v8` crate
  (150.4) has no `v8::Locker`. So a TS system cannot run on whichever pool thread Bevy picks: the
  isolate lives on a thread of its own and the Bevy system hands it the call. That hop is the cost
  of every lane but `main`. Deno's `postMessage` round trip measured 270 µs under heavy load (below); a raw channel hop with no event loop should be far less, and the Rust bench measures it.
- **Bevy already knows how to run TS systems in parallel; we hide them from it.** Today each stage
  is one exclusive system that runs every TS system in turn (`crates/bevy-ts/src/systems.rs`,
  `run_stage`). Bevy 0.20 builds systems with access decided at run time: `QueryParamBuilder` over
  `FilteredEntityMut`, `FilteredResourcesMutParamBuilder`, `Vec<B>` builders and `DynParamBuilder`
  (`bevy_ecs/src/system/builder.rs`, example `ecs/dynamic.rs`). A TS system's declaration already
  names its reads and writes, so each can become one Bevy system with exactly that access. The
  executor then orders and parallelises it against Rust systems and other TS systems, and ambiguity
  detection sees it. Column views stay valid for the call because Bevy guarantees no conflicting
  access runs meanwhile, which is the guarantee the exclusive stage gives today.
- **Shared JS heap state is the real problem, and lanes answer it the way Roblox does.** Roblox's
  Parallel Luau runs scripts under one Actor sequentially in one VM; parallelism comes from more
  Actors, and Actors share only engine data and messages. A per-system isolate would split a
  module's `let score = 0` across copies. A lane per module keeps a module's top-level state in one
  isolate; state two lanes need belongs in a resource or component, where Rust owns it anyway
  (README: "Rust owns the one ECS world"). Every lane loads the whole generation, so imports work
  unchanged; only which lane runs a module's systems differs.
- **Memory is shared already; nothing needs SharedArrayBuffer semantics.** Columns are Rust memory.
  Each isolate wraps the same pointer in its own `ArrayBuffer` for one call, as the bridge does now.
  Two lanes reading one column at once is safe (no writer runs, by Bevy's access rules). V8's shared
  structs are TC39 stage 2 behind a flag: not usable.
- **Writes and structural changes stay a command buffer.** A lane's commands become the Bevy
  system's deferred buffer, applied at the next sync point, as `Commands` are. Today's arena format
  and `apply_output` do not change. Entity ids come from `EntityAllocator::alloc`, which takes
  `&World` and is already thread-safe.
- **The isolate cost is real but bounded: lanes are few and fixed.** An empty Deno worker (a deno_core `JsRuntime` on its own thread) is ready in 7.4 ms; one that compiles 5,000 functions takes 72 ms. A lane count
  near the core count (`main`, render, asset, two to six user lanes) costs about 13 MB each with a mid-sized generation loaded: 100 to 130 MB for eight to ten lanes. Each lane
  compiles the generation on every hot reload; the lanes reload in parallel, so wall time does not
  grow with their number.
- **deno_webgpu cannot wrap Bevy's device.** It pins wgpu-core 29.0.1 (Bevy 0.20 is on 30), needs
  deno_core 0.413, and its only hook is a `wgpu_core::global::Global` put in `OpState`; wgpu 30
  keeps the `Global` and the device and queue ids private (`wgpu/src/backend/wgpu_core.rs`,
  `api/device.rs:21`). `requestDevice` always makes a new device. Getting in means a wgpu fork or a
  full custom wgpu backend. Its JS layer is a shim over its own cppgc classes, so it cannot be
  reused on other ops either.
- **Our own ops over Bevy's device are small and fit Bevy 0.20's render model.** The render graph
  is now a schedule; a system in `RenderGraph` or `Core3d` takes `RenderContext`, whose
  `command_encoder()` is `&mut wgpu::CommandEncoder`, and `RenderDevice::wgpu_device()` is the raw
  device (`bevy_render/src/renderer/render_context.rs`, `render_device.rs:273`). Bevy's own
  `compute_shader_game_of_life` and `custom_post_processing` plug in exactly there. A TS pass is one
  such system that lends the encoder to the render lane for one synchronous call. Estimate: 1.5 to
  2.5k lines of Rust and 0.5k of TS for compute (buffers, WGSL modules, compute pipelines, bind
  groups, dispatch, Bevy images and buffers by handle), about 2k more for render passes.
- **TypeGPU sits on top, not underneath.** `tgpu.initFromDevice({ device })` takes any object shaped
  like `GPUDevice` (0.12.8, 2026-10-16; still 0.x, no 1.0 date). If our ops cover what it calls
  (`createBindGroupLayout`, `queue.writeBuffer`, buffer map and read), a game can use TypeGPU from
  npm with no work from us. The script-materials verdict stands: we do not vendor it. Nobody else
  exposes WebGPU in JS over a host engine's own wgpu device; Servo and Firefox own their `Global`.

## Numbers

Deno 2.9.7 Web Workers, each a deno_core `JsRuntime` on its own thread. Run at load average 40 to
50 (the machine's 18-deep build queue), so timings are upper bounds and the parallel split is
starved of cores. `bench/threading/results/deno-workers.txt`.

| Measure | Result |
|---|---|
| Empty worker, spawn to first message | 7.4 ms |
| Worker that compiles 5,000 functions, spawn to ready | 72 ms |
| Memory per live worker with those 5,000 functions | 13.3 MB (16 workers: 213 MB) |
| `postMessage` round trip, 64 B | 270 µs |
| `postMessage` round trip, 64 KB | 309 µs |
| `pos += vel * dt`, 4M rows, one `SharedArrayBuffer`: 1 / 2 / 4 / 8 workers | 4.40 / 4.74 / 3.68 / 2.56 ms |

What is solid: startup and memory (not CPU-bound). What is not: the hop and the split. Deno's
round trip goes through its event loop and structured clone; lanes would use a blocking channel
and borrowed buffers. The Rust bench (`bench/threading/rust/`, process `bench-threading`) times
that path, and the split on an idle machine; it was still queued for a build slot when this was
written. Its output lands in `bench/threading/results/run.txt`.

## Options

| | Parallel TS systems | Shared state | Cost | Verdict |
|---|---|---|---|---|
| One isolate, exclusive stages (today) | Never | One heap | None | Keep as `main` lane |
| Isolate per Bevy pool thread, any system anywhere | Yes | Module state splits across copies at random | Needs `v8::Locker`, which the crate lacks | No |
| Lanes: isolate per thread, module picks lane, systems as real Bevy systems | Yes, across lanes | A module's state lives in one lane | One hop per call off `main` | Pick |
| Workers with `postMessage` (Web Workers, Deno) | Only as async jobs, not systems | Message copies | Structured clone per message | For asset loaders' shape only |
| Worklets (react-native-worklets copies a closure to another runtime) | Yes | Closure frozen at copy | Babel transform, copies captures | No: lanes load the real module instead |
| Data-only kernels (Unity Burst jobs, Godot group tasks) | Within one system | None | A restricted subset | As `par` over exported functions, on lanes |
| deno_webgpu for TS GPU | n/a | n/a | wgpu fork, version lock with Deno | No |
| Own WebGPU-shaped ops | n/a | n/a | 2 to 4.5k lines | Pick, after lanes |

## Stages

1. **TS systems as Bevy systems, all on `main`.** Build each TS system with `QueryParamBuilder` and
   `FilteredResourcesMutParamBuilder` from its declaration; it runs on the main thread (it holds the
   isolate as non-send data), and its commands go out as a deferred buffer. No parallelism yet, but
   Bevy now orders, reports ambiguity for and traces each TS system, and `run_stage`'s hand-rolled
   loop goes. Build: ours. No new dependency.
2. **Lanes.** A lane is a thread owning a `JsRuntime` that loads the generation; `main` stays the
   main thread. A module declares its lane (`export const lane = "ai"`, or a `bevy-ts.json` map);
   its systems become `Send` Bevy systems that hand their call to the lane and wait. Hot reload
   reloads every lane in parallel. Dev check: a module's systems in two lanes is an error. Build:
   ours, on deno_core as today.
3. **Asset lane.** A TS `AssetLoader` (`loader({ extensions, load(bytes, ctx) })`) runs on a lane
   shared by asset loads; Bevy's async `load` awaits the lane's reply, so no pool thread blocks.
4. **`par` kernels.** `q.par(kernel)` where `kernel` is an exported function: Rust splits the
   query's tables into row ranges and calls the export on N lanes at once. Every lane has the
   module loaded, so no closure crosses. This is the measured 4-way column split below.
5. **Render lane and TS GPU ops.** A lane fed by Bevy's render thread. `renderSystem` and
   `computePass` declarations become systems in `RenderGraph` or `Core3d` sets that lend the
   encoder for one call. A WebGPU-shaped `GPUDevice` facade over Bevy's `wgpu::Device`, compute
   first, aimed at TypeGPU's `initFromDevice`. Build: ours. Reuse: Bevy's `RenderContext`,
   `RenderAssets<GpuImage>`; TypeGPU optional from npm.

Stages 1 and 2 are the fix for "everything runs in one thread"; 3 to 5 each stand alone after 2.

## Risks

- The hop: a lane system that runs many tiny calls a frame loses to `main`. Lanes suit systems
  doing real work; the dev log should show per-lane time.
- Bevy blocks a pool thread while a lane runs a call. With lanes at most the core count this costs
  executor width, not correctness.
- A `NonSend` resource stays `main`-only; TS reading one forces `main`.
- Observers fire during command application (exclusive); a lane's observer is a hop from there.
- On the web there is one engine and threads are Web Workers with `SharedArrayBuffer`; lanes map to
  workers only with cross-origin isolation. iOS and consoles (QuickJS-ng later) get one runtime per
  lane the same way.

## Prior art

- Roblox Parallel Luau: Actors run in parallel, scripts under one Actor sequentially; parallel
  phase reads, writes after `task.synchronize()`. https://create.roblox.com/docs/scripting/multithreading
- Deno Web Workers: a `JsRuntime` and event loop per OS thread, `MessagePort`s, a shared
  `SharedArrayBufferStore`. https://github.com/denoland/deno/blob/main/runtime/web_worker.rs
- Unity: Burst jobs are a restricted subset; managed scripts stay on the main thread.
  https://docs.unity.com/en-us/engine/6000.3/manual/programming-environment/script-compilation/burst
- Godot: scene tree not thread-safe; `WorkerThreadPool.add_group_task` for data work.
  https://docs.godotengine.org/en/stable/tutorials/performance/thread_safe_apis.html
- react-native-worklets: closures serialized to a second runtime; Bundle Mode loads the whole
  bundle there instead. https://docs.swmansion.com/react-native-worklets/docs/bundleMode
- bevy-react: V8 on its own thread for UI, about 1 to 6 ms per hop with its batch sizes.
- V8 pointer compression: all isolates share one 4 GB cage by default (array buffers excluded);
  `IsolateGroup` splits it. https://chromium.googlesource.com/v8/v8.git/+/main/docs/heap/pointer-compression.md
- TC39 shared structs, stage 2: https://github.com/tc39/proposal-structs
- deno_webgpu 0.227.0 (2026-10-21), `Cargo.toml` pins `wgpu-core = "=29.0.1"`:
  https://github.com/denoland/deno/tree/main/ext/webgpu
- TypeGPU `initFromDevice`: https://docs.swmansion.com/TypeGPU/
- Bevy 0.20 source: `bevy_ecs/src/system/builder.rs`, `bevy_render/src/renderer/{mod,render_context,render_device}.rs`,
  `bevy_render/src/pipelined_rendering.rs`, examples `ecs/dynamic.rs`,
  `shader/compute_shader_game_of_life.rs`, `shader/custom_post_processing.rs`.
