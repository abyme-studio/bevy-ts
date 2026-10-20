# What an engine asset preview costs the game (asset-preview, task 4.2)

Measured 2026-10-20 on an M5 Max (18 cores, 128 GB, macOS 26.4). The question: does
previewing a model with the game's own renderer, beside the running game, cost the game more than
abyme's three.js viewer does?

## Method

- **Game:** the arena (`examples/arena`) under the release `bevy-ts dev --viewport`, its game
  viewport attached at 1280×720 points at 2x (2560×1440), ticked at 120 Hz as abyme's display link
  does. Play mode.
- **Model:** `alicia.glb`, a 7.5 MB VRoid character (skinned, textured).
- **Engine preview:** `asset_preview`, then a second viewport at 800×600 points at 2x (1600×1200),
  ticked at 120 Hz.
- **three.js:** abyme's viewer (`web/plugins/src/viewers/Model.tsx`, three 0.186.1) standalone in
  `bench/asset-preview/`, the same model in a headless Chromium (abyme's browser) on Metal through
  ANGLE. Headless Chrome throttles `requestAnimationFrame` to ~10 Hz, so it draws on a timer at
  120 Hz, at 1600×1200 (`?hz=120&dpr=2`), with the camera turning so every frame is new work.
- **Harness:** `crates/bevy-ts-cli/tests/preview_bench.rs` via `scripts/preview-bench.sh`, one engine
  per scenario: 4 s warm-up, then 10 s measured. `viewport_stats` gained `frameTimes` (CPU: the
  engine's update time; GPU: each frame from submit to GPU completion, so queue wait is included)
  and `gpuAllocatedBytes` (Metal `currentAllocatedSize`), over the last 1000 frames. RSS and CPU
  come from `ps`, footprint (which counts Metal memory) from `footprint`.
- **Caveat:** the bevy-studio implementer was building abyme on the same machine. Run 1 (frame
  times below) was quiet; run 2 (the footprint column) was not: its no-preview game fell to 112 fps
  with a p99 update of 18 ms in that run alone.

## Results

Frame times in ms; the game delivered 119-121 fps in every scenario of run 1.

| Scenario | Update p50 / p99 | Game GPU p50 / p99 | Preview GPU p50 | Engine CPU | RSS | Metal alloc | Footprint (run 2) |
|---|---|---|---|---|---|---|---|
| No preview | 1.38 / 3.28 | 0.59 / 0.78 | - | 59% | 191 MB | 57 MB | 490 MB |
| Engine preview, visible | 1.66 / 3.71 | 1.20 / 1.52 | 1.19 | 87% | 335 MB | 232 MB | 866 MB |
| Engine preview, hidden | 1.46 / 2.34 | 0.56 / 0.76 | - | 62% | 310 MB | 111 MB | 655 MB |
| three.js beside the game (engine side) | 1.59 / 3.92 | 0.42 / 0.72 | - | 63% | 191 MB | 57 MB | - |
| Idle, play (no viewport visible) | 1.59 / 4.01 | - | - | 28% | 185 MB | 12 MB | 266 MB |
| Idle, edit (as the studio keeps it) | 2.14 / 27.65 | - | - | 10% | 212 MB | 120 MB | 494 MB |

The three.js viewer itself, in the browser, drawing the model at 120 Hz: **+628 MB** footprint
(its GPU process +535 MB, the page's renderer +108 MB) and about **19% CPU** across the browser's
processes, against the same browser on a blank page.

## Reading

- **GPU:** a visible engine preview draws in the game's frame, so the game's GPU time doubles,
  +0.61 ms p50 and +0.74 ms p99 for this model, 1.2 ms in all against an 8.3 ms frame at 120 Hz.
  three.js renders outside the game's frame: the game's GPU time did not rise (0.42 against 0.59,
  noise in the other direction). On a GPU-bound game the preview's cost lands on the game's frame;
  three.js's lands on the GPU's other queue.
- **CPU:** the preview adds 0.28 ms to the game's update p50 (extraction and visibility for one
  more camera) and 28 points of engine CPU; three.js costs about 19 points of CPU in the browser
  and nothing in the engine.
- **Memory:** the engine preview adds 376 MB of footprint (144 MB RSS, 175 MB Metal) while visible;
  three.js adds 628 MB to the browser. Hidden, the preview keeps 165 MB (the model stays loaded
  until the preview closes, 30 s after its viewport detaches) and costs no frame time.
- **Idle:** with no viewport visible the engine costs 10% CPU in edit mode and 28% in play (play
  keeps simulating at 60 Hz). Measuring this found a bug: a hidden viewer's display-link ticks
  still woke the engine for a full update each, 100% CPU in edit mode. A hidden or detached
  viewport's ticks now pace nothing (`Shared::leads`, test `a_hidden_viewports_ticks_pace_nothing`).
  The edit-mode p99 of 27 ms is the 100 ms idle poll's occasional update doing reload and asset
  work, not a frame anyone sees.

## Not measured

- abyme's real three.js pane: it composites a WebGL canvas into abyme's window, which the headless
  browser does not, so three.js's real GPU cost is somewhat higher than here. Measuring it needs the
  live abyme, which this change's tests do not touch.
- A GPU-bound game. The arena uses 0.6 ms of an 8.3 ms frame, so neither the preview nor three.js
  could make it miss a frame here.
