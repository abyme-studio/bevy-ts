# Script fullscreen pass at 1920×1080 (script-fullscreen-pass, design 8)

Apple M5 Max, 128 GB, macOS 26.4, 2026-10-29. The machine was shared with other builds, at load
101 to 116. Command: `bench/fullscreen-pass/run.sh 8` (the `fullscreen-pass-bench-8` process), at
commit 6bbf72f9f; the raw lines are in `runs.txt`. One `Camera2d` under `PixelCamera` zoom 3 draws
2,000 atlas sprites into a 1920×1080 image. Rendering is not pipelined, and each frame waits for
the GPU. 600 frames after 120 warm-up, with the arms alternated. The estimate is 0.15 to 0.6 ms
for the pass.

| ms, frame time, 8 runs | off | passthrough | grade |
|---|---|---|---|
| median of the run medians | 6.16 | 6.45 | 5.03 |
| lowest run median | 4.27 | 4.26 | 3.95 |
| highest run median | 22.86 | 25.37 | 44.15 |
| pass span, CPU, median | none | 0.001 | 0.001 |

- Not settled. The arms are within noise of each other, even at 8 runs. Run medians swing 4 to 44
  ms in every arm, `off` included, which is the machine's load. A 0.15 to 0.6 ms pass cannot show
  through that. Even the quietest run of each arm (4.27, 4.26, 3.95 ms) puts no pass above `off`.
- Metal gives no GPU timestamps, so `RenderDiagnosticsPlugin` reports only the pass span's CPU
  time: about 1 µs, the cost of encoding one draw. The GPU time is measurable only in the frame
  time, which is why each frame waits for the GPU.
- Re-run on a quiet machine (load under 10) to place the pass against the estimate:
  `abyme call processes start name=fullscreen-pass-bench`.
- The first 3-run batch in `runs.txt` built against two `bevy_render` versions and is discarded.
  `Cargo.lock` now follows the workspace's (Bevy 0.20.0, patched).
