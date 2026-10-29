# Script 2D materials at 1920×1080 (script-material2d, design 8)

Apple M5 Max, 128 GB, macOS 26.4, 2026-10-30 08:33 to 08:35, at load 8.6 falling to 5.3 (1-min
`uptime` before and after). Command: `bench/material2d/run.sh 8` (the `material2d-bench-8`
process), at commit a91603f80; the raw lines are in `runs.txt`. One `Camera2d` under `PixelCamera`
zoom 3 draws 48-art-px planets of three layers (sea, land, cloud; a ring on every fifth) into a
1920×1080 image; `one` is a single 200-art-px planet. Rendering is not pipelined, and each frame
waits for the GPU. 600 frames after 120 warm-up, with the arms alternated.

| ms, 8 runs | off | one | n20 | n100 | n200 | n500 |
|---|---|---|---|---|---|---|
| frame, median of the run medians | 1.25 | 1.95 | 2.14 | 2.52 | 3.32 | 4.87 |
| frame, lowest run median | 1.11 | 1.33 | 1.43 | 1.85 | 2.06 | 2.41 |
| frame, highest run median | 1.36 | 2.04 | 3.09 | 5.21 | 5.99 | 6.01 |
| frame p95, median of the run p95s | 1.47 | 2.23 | 2.51 | 4.49 | 5.47 | 5.92 |
| transparent 2D pass, CPU, median | none | 0.001 | 0.008 | 0.031 | 0.057 | 0.129 |
| opaque 2D pass, CPU, median | 0.000 | 0.002 | 0.002 | 0.002 | 0.002 | 0.003 |

- Gate (`n100` under `off` + 2 ms, at load under 10): met. `n100` reads 1.27 ms over `off`
  (median of the run medians), within the 1.6 ms estimate.
- Estimate (proposal, from the learn-planets research): 100 planets at most 1.6 ms of GPU, 500 at
  most 8 ms. `n500` reads 3.6 ms over `off`.
- Two runs of each of `n100` and `n200`, and four of `n500`, read 5 to 6 ms; the rest read 1.9 to
  4.1 ms. `off` held 1.1 to 1.4 ms throughout. The median places the gate; the slow runs are
  noted, not explained.
- CPU: the transparent pass's encoding grows with the draws, about 0.08 µs a layer (0.13 ms at
  500 planets, about 1,600 layers). Metal gives no GPU timestamps, so GPU time shows only in the
  frame time.
- `n200` is recorded for learn-planets and gates nothing.
- Earlier batches in `runs.txt` decide nothing: 2026-10-29 at load 150 to 165 (3 runs, `n100`
  6.3 ms over `off`), and 2026-10-30 06:32 at load 40 to 48 (2 runs, from a build-only start:
  `seq 0` on macOS counts 1, 0).
