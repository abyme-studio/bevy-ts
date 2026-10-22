# What the orientation gizmo costs (2026-10-22)

The first build below drew the gizmo with a second camera every frame. The operator ruled it be
redrawn only when the view changes; the second section measures that build, the one that landed.

The orientation gizmo (change `viewport-tools`) adds one small camera in edit mode: an
orthographic `Camera3d` over a 96 px corner of the game viewport, on render layer 32, which no
light shares (so Bevy builds no shadow maps for it), drawing three unlit discs and a few gizmo
lines. Nothing in play.

## How it was measured

The process `viewport-tools-ab` runs `crates/bevy-ts-cli/tests/viewport_pacing.rs` (see
`viewport-pacing.md`) on the release engine: the arena in edit mode, a 480×320 hole at 2x ticked at
120 Hz. It alternates `BEVY_TS_VIEWPORT_ORIENT=0` (the gizmo left out) and the default, so both arms
see one machine load. Two batches, 20 runs; 16 completed, 8 an arm. The other 4 failed before edit
mode with no frames in play (3 without the gizmo, 1 with), a startup flake of the test, not
of either arm.

Apple M5 Max, load average 6 to 38 from other agents' builds. "CPU" is the engine's update time
from `viewport_stats`; "engine CPU" is the process's CPU from `ps` (all threads, render thread
included).

## Results

Medians of the 8 runs per arm:

| phase | gizmo | fps | engine CPU | CPU p50 | CPU p99 | GPU p50 | GPU p99 |
|---|---|---|---|---|---|---|---|
| edit at rest | off | 120 | 116% | 1.88 ms | 9.25 ms | 1.17 ms | 5.00 ms |
| edit at rest | on | 119 | 133% | 2.03 ms | 10.50 ms | 1.37 ms | 4.89 ms |
| pointer moves | off | 119 | 108% | 2.52 ms | 13.84 ms | 1.33 ms | 4.12 ms |
| pointer moves | on | 120 | 133% | 2.25 ms | 4.28 ms | 1.91 ms | 3.88 ms |

- Frames a second hold at the display's 120 in both arms.
- GPU p50 rises 0.2 to 0.6 ms: the second view's passes.
- Update time (CPU p50) moves by under 0.3 ms either way, within the noise; the p99s follow the
  machine's load, not the arm.
- The engine's process CPU rises about 17 to 25 points (of one core): the render thread preparing
  and submitting a second view each frame.

Raw lines: `target/viewport-tools-ab-1.log` and `target/viewport-tools-ab-2.log` (not kept in git).

## Redrawn only on change (the landed build)

The corner camera now renders into a texture only on a frame where the editor camera turned; a
quad just in front of the editor camera shows the texture every frame. (After this measurement the
redraw became two frames, since gizmo lines land a frame late, and the first draw in a process
became a 1.5 s warm-up while Bevy compiles the pipelines; a still view is unchanged.) Same
bench and process, 13 alternated pairs. 16 runs reached edit mode, 9 without the gizmo and 7 with it.
The other 10 failed before edit mode with no frames in play, 4 without and 6 with. Load average 7
to 33 from abyme's release builds. Every completed run also failed after the measured phases, in
both arms: the stats call after "edit hidden" errored (`viewport_pacing.rs:117`), which the first
measurement did not show. That is outside the numbers here.

Medians, all completed runs:

| phase | gizmo | runs | fps | engine CPU | CPU p50 | CPU p99 | GPU p50 | GPU p99 |
|---|---|---|---|---|---|---|---|---|
| edit at rest | off | 9 | 120 | 144% | 2.52 ms | 7.40 ms | 1.66 ms | 4.52 ms |
| edit at rest | on | 7 | 120 | 165% | 2.77 ms | 8.38 ms | 1.77 ms | 6.55 ms |
| pointer moves | off | 9 | 120 | 139% | 2.35 ms | 4.35 ms | 1.88 ms | 3.32 ms |
| pointer moves | on | 7 | 120 | 164% | 2.95 ms | 9.53 ms | 1.94 ms | 6.33 ms |

Medians of the runs at a 1-minute load average under 15 (3 an arm):

| phase | gizmo | fps | engine CPU | CPU p50 | CPU p99 | GPU p50 | GPU p99 |
|---|---|---|---|---|---|---|---|
| edit at rest | off | 121 | 137% | 2.26 ms | 4.52 ms | 1.65 ms | 3.31 ms |
| edit at rest | on | 120 | 143% | 2.17 ms | 4.82 ms | 1.61 ms | 6.66 ms |
| pointer moves | off | 120 | 132% | 2.28 ms | 2.89 ms | 1.82 ms | 2.62 ms |
| pointer moves | on | 120 | 147% | 2.37 ms | 2.93 ms | 1.84 ms | 2.76 ms |

- At rest, the light-load runs put the gizmo within noise of the off arm: GPU p50 1.61 against
  1.65 ms, update time 2.17 against 2.26 ms. The earlier build cost 0.2 to 0.6 ms of GPU a frame.
- Pointer moves do not turn the view, so they redraw nothing; the 15-point process-CPU gap there
  is within the spread of single runs (131% to 185% across the batch).
- The medians over all runs differ more, but the two arms' runs did not see the same load.

Operator's go on this cost: 2026-10-22, via lead@bevy-ts.
