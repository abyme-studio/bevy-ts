# Pixel-art 2D: the pass at 10,000 sprites (pixel-art-2d, design 8)

Apple M5 Max, macOS 26.4, 2026-10-28. Machine shared with other builds (load 52 to 54). Command:
`bench/pixel-2d/run.sh 3` (the `pixel-2d-bench` process), at commit 5ff7312de; raw lines in
`runs.txt`. 10,000 sprites, each with a shadow child, each holding a `Mesh2d`, under
`MinimalPlugins`, Bevy's transform propagation and `Pixel2dPlugin`; no renderer. 600 frames after
120 warm-up, arms alternated. The gate: the pass under 0.5 ms at 10,000 sprites.

| ms, 3 runs: median of each run | off | ysort-still | ysort-moving | all-moving |
|---|---|---|---|---|
| pass (`Pixel2dSystems`) | 0.015, 0.013, 0.018 | 0.169, 0.169, 0.170 | 0.174, 0.175, 0.185 | 0.181, 0.192, 0.180 |
| re-queue of a moved z | 0.001, 0.001, 0.001 | 0.019, 0.019, 0.019 | 0.093, 0.093, 0.091 | 0.095, 0.100, 0.093 |
| camera snap | 0.001 | 0.001 | 0.001 | 0.002, 0.003, 0.002 |
| propagation (Bevy's) | 0.489, 0.357, 0.405 | 0.095, 0.098, 0.100 | 0.422, 0.389, 0.699 | 0.451, 0.489, 0.752 |

- **The gate holds**: the pass is 0.17 to 0.19 ms with every sprite y-sorted, moving, a tenth
  `ScreenSize`, under an easing, moving `PixelCamera`; with the re-queue, 0.28 ms.
- A still y-sorted world costs 0.17 ms a frame: a read and a compare per sprite, and no write, so
  propagation stays at its still cost (0.1 ms against 0.4 ms moving).
- The re-queue (`requeue_new_z`) costs 0.09 ms with 10,000 moving sprites and their shadows. Its
  cost in Bevy's render world (each marked mesh specialized and queued again) is not measured
  here: there is no renderer.
- The arms differ far past the noise, so 3 runs settle it. The p95s (0.4 to 4.7 ms for the pass,
  up to 10 ms for propagation, in every arm, `off` included) are the machine's load, not the pass.
