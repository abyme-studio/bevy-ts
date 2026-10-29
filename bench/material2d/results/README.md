# Script 2D materials at 1920×1080 (script-material2d, design 8)

Apple M5 Max, 128 GB, macOS 26.4, 2026-10-29. The machine was shared with other builds, at load
150 to 165. Command: `bench/material2d/run.sh 3` (the `material2d-bench` process), at commit
89d6f505b; the raw lines are in `runs.txt`. One `Camera2d` under `PixelCamera` zoom 3 draws
48-art-px planets of three layers (sea, land, cloud; a ring on every fifth) into a 1920×1080
image; `one` is a single 200-art-px planet. Rendering is not pipelined, and each frame waits for
the GPU. 600 frames after 120 warm-up, with the arms alternated. 3 runs per arm: the operator
asked for a result at whatever load there was, not 8 runs on a quiet machine.

| ms, 3 runs | off | one | n20 | n100 | n200 | n500 |
|---|---|---|---|---|---|---|
| frame, median of the run medians | 14.00 | 15.33 | 14.73 | 20.29 | 20.12 | 25.92 |
| frame, lowest run median | 11.19 | 14.51 | 12.29 | 17.69 | 18.17 | 22.09 |
| frame, highest run median | 17.98 | 16.54 | 16.60 | 27.76 | 27.17 | 27.82 |
| frame p95, median of the run p95s | 44.64 | 45.94 | 52.99 | 57.77 | 50.63 | 56.04 |
| transparent 2D pass, CPU, median | none | 0.006 | 0.023 | 0.072 | 0.13 | 0.30 |
| opaque 2D pass, CPU, median | 0.001 | 0.014 | 0.014 | 0.015 | 0.015 | 0.015 |

- Gate (`n100` under `off` + 2 ms, at load under 10): not met, and not decided. `n100` reads
  6.3 ms over `off` (median of the run medians; 9.1, 9.8 and 3.7 ms run by run), but the run was
  at load 150 to 165, where the spec says a run decides nothing. `off` alone swings 11 to 18 ms
  between runs.
- Estimate (proposal, from the learn-planets research): 100 planets at most 1.6 ms of GPU, 500 at
  most 8 ms. Under this load the frame time cannot place it: `n200` reads the same as `n100`, and
  `n500` only 5.6 ms above it.
- CPU: the transparent pass's encoding grows with the draws, about 0.6 µs a layer (0.30 ms at 500
  planets, about 1,600 layers). Metal gives no GPU timestamps, so GPU time shows only in the frame
  time.
- `n200` is recorded for learn-planets and gates nothing.
- Re-run on a quiet machine (load under 10) to decide the gate:
  `abyme call processes start name=material2d-bench`.
