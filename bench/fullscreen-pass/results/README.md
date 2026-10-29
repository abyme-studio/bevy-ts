# Script fullscreen pass at 1920×1080 (script-fullscreen-pass, design 8)

Apple M5 Max, 128 GB, macOS 26.4, 2026-10-30 08:35, at load 4.4 to 4.5 (1-min `uptime` before
and after). Command: `bench/fullscreen-pass/run.sh 8` (the `fullscreen-pass-bench-8` process), at
commit a91603f80; the raw lines are in `runs.txt`. One `Camera2d` under `PixelCamera` zoom 3 draws
2,000 atlas sprites into a 1920×1080 image. Rendering is not pipelined, and each frame waits for
the GPU. 600 frames after 120 warm-up, with the arms alternated. The estimate is 0.15 to 0.6 ms
for the pass.

| ms, frame time, 8 runs | off | passthrough | grade |
|---|---|---|---|
| median of the run medians | 1.56 | 1.53 | 1.49 |
| lowest run median | 1.40 | 1.45 | 1.45 |
| highest run median | 1.63 | 1.83 | 1.80 |
| p95, median of the run p95s | 2.06 | 1.89 | 1.82 |
| pass span, CPU, median | none | 0.000 | 0.000 |

- The pass does not show in the frame time: `passthrough` and `grade` read within 0.1 ms of
  `off`, inside the run-to-run spread of every arm (0.2 to 0.4 ms). Its GPU cost is under the
  noise, below the 0.15 ms low end of the estimate.
- Metal gives no GPU timestamps, so `RenderDiagnosticsPlugin` reports only the pass span's CPU
  time: under 1 µs, the cost of encoding one draw.
- Earlier batches in `runs.txt` decide nothing: 2026-10-29 at load 101 to 116 (8 runs, run
  medians 4 to 44 ms in every arm), and 2026-10-30 06:31 at load 39 to 45 (2 runs, from a
  build-only start: `seq 0` on macOS counts 1, 0). The first 3-run batch of 2026-10-29 built
  against two `bevy_render` versions and is discarded.
