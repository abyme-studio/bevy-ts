# update-stall: why Update stops after "generation applied"

## Cause

The GPU driver stops accepting this process's work: Metal's own submit thread blocks inside the
kernel, the queue fills with command buffers that never complete, and the next render, then the
next Update, block for good. Not a lock, a pipeline compile or a script wait in bevy-ts.

"generation applied" is only the last line before the first real renders. It is not the trigger.

## Evidence

A hung `it` test binary (pid 79137, `screenshot_ready::a_looping_clip_on_a_slow_gltf...`),
sampled four times over 80 s, 35 min into the hang (`/tmp/it-e8f0dc3a1a2133dc_*.sample.txt`).
Every sample has the same stacks:

| Thread | Stuck in |
|---|---|
| `com.Metal.CommandQueueDispatch` (Apple's) | `-[_MTLCommandQueue _submitAvailableCommandBuffers]` → `IOGPUCommandQueueSubmitCommandBuffers` → `IOConnectCallMethod` (a kernel call into the GPU driver) |
| 5 compute-pool threads + the test thread | `wgpu_core CommandEncoder::finish` → wgpu-hal `begin_encoding` → `-[MTLCommandQueue commandBuffer]` → `semaphore_wait` |
| Render thread | `bevy_core_pipeline::submit_pending_command_buffers`, waiting on those encoders |
| Main (test) thread | `bevy_render::pipelined_rendering::renderer_extract`, waiting for the render app to come back |

The chain:

1. Apple's submit thread is in the driver and does not return. Nothing more reaches the GPU, so
   nothing completes.
2. wgpu makes the queue with room for 4096 live command buffers
   (`wgpu-hal-30.0.1/src/metal/adapter.rs:48`, `:78`). Frames go on being encoded and committed
   until that room is gone.
3. Then `-[MTLCommandQueue commandBuffer]` blocks (`wgpu-hal-30.0.1/src/metal/command.rs:503`).
   wgpu's guard (`command.rs:485-500`) counts only buffers created and not yet submitted, so
   buffers stuck behind the driver never trip it and it never reports device lost.
4. The render thread waits on those encoders. The main thread waits for the render app in
   `renderer_extract`, so `app.update()` never returns: no Update, no `First`, no ready count, no
   timeout of ours (they all run inside Update).

Agent town's failure fits: the engine logs the frame after the apply, then nothing. Neither the
60 s "not ready, shooting anyway" warning (`crates/bevy-ts-remote/src/screenshot.rs:334-353`) nor a
shot appears in 120 s, so Update had stopped. No sample was taken of it, so that is inferred.

## GPU restarts on this Mac

`/Library/Logs/DiagnosticReports{,/Retired}/gpuEvent-*.ips`: 26 GPU restarts since boot
(`ioreg` `recoveryCount` = 28), every one a "BIF0 page fault": the GPU read memory not mapped,
at address 0 or a small offset (0, 64, 128, 320, 57472, 114688). The process blamed is a
bevy-ts engine (`bevy-ts`, `material-params-…`, `animation_bench_…`) or `<unknown>`. A restart
resets the GPU for every process on the machine.

They line up with some hangs, not all:

| Hang | Window | GPU restart in it |
|---|---|---|
| ambient_light (ts-heap-fields-2) | about 02:10-02:51, 2026-10-27 | 02:16:19, `bevy-ts` |
| agent town civ-meaning-after | 02:18:17-02:20:17 | none (02:16:19 is 2 min before) |
| screenshot_ready (sampled) | 22:00-22:36, 2026-10-26 | none |

So the restarts are a second bevy-ts GPU bug (a null or freed buffer read on the GPU), and may
cause some stalls. They do not explain the sampled one.

## What it is not

- Not a pipeline compile: no thread is in a shader or pipeline compile.
- Not a bevy-ts lock or a script wait: V8 threads idle; no bevy-ts frame on any blocked stack.
- Not the vendored bevy_render async-pipelines patch, as far as the evidence goes: upstream kept
  macOS synchronous only for speed (bevy#10812, recalled in bevy#26151). Not ruled out for the
  page faults, since every engine since 2026-10-24 has the patch and no older reports exist.

## Repro

The driver is `/tmp/update-stall-repro/run.mjs` (outside the repo). It starts K agent-town
engines at once, R rounds. Each engine is a copy of agent-town's `src`, `assets` and config, run
with `bevy-ts dev game.ts --headless`, `BEVY_TS_SCREENSHOT` and a dead `ABYME_URL`. It records
GPU `recoveryCount` around each round. An engine with no shot in 150 s is sampled and its process
group killed.

```sh
BIN=<agent-town>/node_modules/.pnpm/@bevy-ts+darwin-arm64@0.1.0-alpha.32/node_modules/@bevy-ts/darwin-arm64/bin/bevy-ts \
  node /tmp/update-stall-repro/run.mjs <agent-town worktree> <K> <R> <stress 0|1>
```

`BIN` must be the engine binary. `node_modules/.bin/bevy-ts` is a node launcher: killing it leaves
the engine running, and `sample` sees only node.

Results, 2026-10-27, M5 Max, load already 150-190 from other agents (no `yes` added):

| K × R | Load | Shot | Hung | GPU restarts during the runs |
|---|---|---|---|---|
| 6 × 3 | 149-165 | 18 | 0 | 0 |
| 12 × 3 (of 5; the driver was killed in round 3) | 165-194 | 24 in rounds 1-2; round 3's other 10 not recorded | 2 | 0 |
| 2 × 10 (an abyme process) | 112-140 | 20 (3-9 s each) | 0 | 0 |

Both hung engines' logs end exactly like the land check's: `generation 1 applied`, the camera
warning, the signal warning, then nothing for 150 s. Shots took 5-64 s as load rose. Their stacks
were not captured, since the sample hit the launcher.

So: the hang comes back at load ~190 with 12 engines at once, about 1 in 20. Lower load or fewer
engines did not hang in 38 runs. No agent-town engine stack exists yet: the 12-engine run that
hangs is too heavy for a shared Mac, and the 2-engine run never hangs. The chain above comes from
the sampled test binary; that agent town hangs on the same chain is inferred from its log.

## Fix recommended

Bound the stall in the engine; it cannot be fixed from inside the process.

1. A render watchdog in bevy-ts: a thread that watches a frame counter the main world bumps in
   `First`. When no frame finishes in 30 s (wall time), it logs the stuck GPU submit plainly ("the
   GPU driver has not taken this engine's work for 30 s") and exits the process with an error.
   Under `--headless`, `BEVY_TS_SCREENSHOT` and tests that is right: the run fails fast and the
   land check reruns, instead of 120 s of silence or a 41 min hang. A windowed `play` warns only.
   Test binaries get the same through the existing `common::watchdog` (f39e9268b,
   1d7438aca), which should become the default for every render test, not one test at a time.
2. Fewer GPU clients at once: count a headless render (engine or test binary) as a heavy run under
   gate-slot, so the Mac does not run 70 GPU processes at load 150.
3. Separately, find the page fault: run an engine with Metal API and shader validation
   (`MTL_DEBUG_LAYER=1`, `MTL_SHADER_VALIDATION=1`) on the examples and agent town until it
   reports the bad read. That is a change of its own.

Confidence: high on the chain inside the process (four samples, same stacks). Low on why the
driver blocks: there is no kernel log for it, and the restarts explain only one of three hangs.
