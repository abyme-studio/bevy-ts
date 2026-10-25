# World text: 1,000 labels (world-text, task 4.1)

Apple M5 Max, Metal, macOS 26.4, 2026-10-25. Machine shared with other builds (load 24 to 54).
Command: `bench/world-text/run.sh 8` (the `world-text-bench` process, `WT_RUNS=8`), at commit
482bc6e3; raw lines in `runs.txt`. 1,000 labels from Rust (distinct names, two colours, one
outline) under a `Camera3d` at 1280×720 offscreen, pipelining off, 600 frames after 240 warm-up,
arms alternated. Bevy records no GPU timestamps on Metal, so "GPU synced" is the frame time with
each frame waited out on the GPU.

| ms, 8 runs: median (fastest) | empty | still | churn |
|---|---|---|---|
| first mount | 4.6 (3.9) | 51 (31) | 45 (37) |
| frame median | 4.4 (3.9) | 6.3 (3.6) | 9.3 (7.8) |
| frame p95 | 16.8 (4.9) | 22.0 (4.8) | 28.0 (10.4) |
| GPU synced median | 9.4 (4.8) | 9.0 (5.3) | 13.5 (10.7) |

Every run of every arm: labels laid out per frame 0 (still) and 100 (churn); one material; one
atlas of 4096×256 (1 MB) holding 46 glyphs; every label drawn on the first frame after spawn.

- **Still**: 1,000 labels that do not change cost no more than an empty scene beyond the noise
  (the fastest runs: 3.6 against 3.9 ms; GPU synced 5.3 against 4.8 ms). A still frame lays out
  no label.
- **Churn**: 100 labels given new text a frame cost about 4 ms a frame on the CPU (the fastest
  runs: 7.8 against 3.9 ms), about 40 µs a label: layout, mesh and upload.
- **Mount**: 1,000 labels lay out, fill the atlas and draw in one frame, 31 to 220 ms under this
  load.
- The load made medians and p95 swing between runs (the empty arm's p95 from 4.9 to 66 ms), so
  the fastest run of each arm is the fairer comparison. An earlier 3-run round, before the
  shader's four taps a pixel, gave frame medians of 4.0 (empty), 4.0 (still) and 7.9 ms (churn).
