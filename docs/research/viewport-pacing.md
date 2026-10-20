# Where the viewport's frames go (2026-10-20)

The bevy-studio walk measured the release engine (03f07c7, protocol 1.9) on the arena in edit mode
in abyme.app on a 120 Hz display at ~39 fps, 95 dropped ticks, GPU 1.56 ms, input to frame median
33.4 ms and p95 170 ms. The budget (shared-surface "The viewer's display paces frames", "Latency is
measured and bounded") is frames at the display's rate, no skips at rest, and key to frame within
two refreshes (16.7 ms).

## How it was measured

`crates/bevy-ts-cli/tests/viewport_pacing.rs` (process `viewport-pacing`) stands in for abyme
without abyme: it copies `examples/arena`, runs `bevy-ts dev --viewport` (release), attaches a
480×320 hole at 2x and 120 Hz, ticks it from a thread every 8.33 ms, and runs five phases: play at
rest (2 s), edit settling (1 s), edit at rest (3 s), keys (~6 a second, 3 s) and pointer moves
(one every 4 ms, 3 s). Each phase prints frames a second, skips and dropped ticks from
`viewport_stats`, the gaps between `frame` messages, update (CPU) and GPU time, and input to frame.

`BEVY_TS_VIEWPORT_TRACE=<file>` (`crates/bevy-ts-viewport/src/trace.rs`) makes the engine write a
per-frame timeline: each tick and whether it was dropped, each update's start and end and why it
woke, the slot picked at extract, the submit and the GPU's completion. `PACING_SAMPLE=<file>` has
the test run `sample` on the engine over edit at rest. The process `viewport-pacing-ab` runs the
test six times, alternating `BEVY_TS_VIEWPORT_PACING=0` (the pacing before this note: default QoS,
a tick during an update dropped) and the current pacing, so both see one machine load. (The
experiment below toggled QoS alone, with a knob since folded into this one.)

The test's viewer does not show its surfaces, so the window server never holds a slot: skips from
a slot in use are not reproduced here (abyme saw 2). Not measured in abyme.app itself.

## What the timeline shows

The machine was never quiet: Apple M5 Max, 18 cores (6 performance), load average 35 to 67 from
other agents' builds throughout. The first run, beside viewport-stream-input's cargo build, gave
12 fps in edit at rest and updates of 10 to 90 ms; later runs at load ~50 gave 33 to 84 fps.

- GPU time is 0.5 to 1.5 ms a frame: rendering is not the cost.
- With pipelined rendering, `App::update` waits for the render thread to hand the render world
  back before it extracts. In the trace, update start to extract is the previous frame's
  extract-to-submit on the render thread, 20 to 90 ms under the heaviest load.
- The `sample` profile over 3 s of edit at rest: the main thread spends 878 of 1247 samples in
  `update`, but 429 of those parked waiting for the compute pool's systems and 241 waiting for the
  render world; the render thread spends 744 of its 1161 busy samples parked waiting for its own
  systems on the compute pool. The systems themselves (gizmo prepare, uniforms, `queue_submit`,
  `command_encoder_finish`, transforms, the edit gizmos) add up to a few ms a frame. The engine's
  CPU per frame is small; the frame is a chain of handoffs between the main thread, the render
  thread and ten compute threads, each of which, at default QoS on a saturated machine, waits its
  turn behind the compilers.
- After an update that takes over 8.33 ms, the runner waits for the next tick (the spec drops a
  tick that arrives mid-update): a 9 ms update costs two refreshes, so fps halves rather than
  degrading by the overrun. The trace shows up to 8.3 ms idle before most updates.

## The QoS experiment

Putting the main thread, the render thread and the compute pool in `QOS_CLASS_USER_INTERACTIVE`
(as an app's main and render threads are), viewport mode only, alternated with default QoS at the
same load (load 35 to 62):

| phase | default QoS fps (3 runs) | user-interactive fps (2 runs) |
|---|---|---|
| play at rest | 80 / 97 / 91 | 101 / 110 |
| edit at rest | 49 / 81 / 84 | 97 / 88 |
| edit keys | 55 / 100 / 88 | 97 / 103 |
| edit pointer moves | 57 / 88 / 87 | 106 / 99 |

| | default QoS | user-interactive |
|---|---|---|
| update p50 | 4.3 to 7.7 ms | 2.2 to 4.1 ms |
| update p99 | 30 to 240 ms | 11 to 29 ms |
| key to frame median | 17.6 to 43 ms | 18.5 to 19 ms |
| pointer to frame median, p95 | 21 to 41, 48 to 83 ms | 19, 36 to 46 ms |

Those runs ticked from a default-QoS thread, which itself jitters under load; a display link's
thread does not. Rerun with the test's ticker at user-interactive, load 25 to 43, three runs each:

| phase | default QoS fps | user-interactive fps |
|---|---|---|
| play at rest | 114 / 106 / 81 | 113 / 110 / 116 |
| edit at rest | 72 / 117 / 77 | 119 / 103 / 115 |
| edit keys | 69 / 118 / 56 | 114 / 105 / 106 |
| update p99, edit | 22 to 44 ms | 12 to 19 ms |
| key to frame median | 16.5 to 40 ms | 18.2 to 18.4 ms |

Default QoS reaches 117 fps only in the run where the load dipped; user-interactive stays above
100 fps in every run and its tail is half as long. Neither meets the 16.7 ms key-to-frame budget
under this load: the remaining dropped ticks (5 to 50 per 3 s) are updates past 8.33 ms, each
costing a whole refresh, which is what proposal B addresses.

## The proposals as sent


A: user-interactive QoS for the engine's frame threads in viewport mode. No extra CPU; the engine
outranks default-QoS work (builds, other agents) while it runs.

B: latch one tick that arrives during an update and run the next update as soon as it ends,
instead of dropping it (a change to "The viewer's display paces frames"). A 9 ms update would give
~110 fps instead of 60; an overrun frame lands up to one update late instead of on the next vsync.

Both were the operator's to rule on; sent to bevy-ts-dx on 2026-10-20.

## Rulings (operator, 2026-10-20)

- A, yes, scoped: the main, render and compute-pool threads are user-interactive only while a
  viewport is attached and visible, and back at their default class when every viewport is hidden
  or detached, and outside viewport mode. `crates/bevy-ts-viewport/src/qos.rs`: each thread
  registers itself (the runner, the render world's first frame, the compute pool's
  `on_thread_spawn`), and the runner puts a QoS override on every registered thread when
  `any_rendering` turns on and ends them when it turns off.
- B, yes: a tick that arrives during an update is kept and starts the next update as soon as that
  one ends; a second before then is dropped (`Wake::keep_tick`). shared-surface "The viewer's
  display paces frames" says so, with the scenario "An update overruns a refresh".

## After

`viewport-pacing-ab`, the ticker at user-interactive, alternating `BEVY_TS_VIEWPORT_PACING=0`
(before) and the current pacing (after), load 34 to 69, three runs each:

| phase | before fps | after fps |
|---|---|---|
| play at rest | 100 / 56 / 93 | 121 / 124 / 121 |
| edit at rest | 92 / 51 / 110 | 112 / 120 / 121 |
| edit keys | 84 / 59 / 85 | 107 / 121 / 120 |
| edit pointer moves | 87 / 48 / 89 | 111 / 121 / 121 |

| | before | after |
|---|---|---|
| dropped ticks, edit at rest (3 s) | 91 / 212 / 33 | 25 / 0 / 0 |
| skipped, all visible phases | 0 | 2 (one slot pick each, two runs) |
| update p99, edit | 30 to 104 ms | 5 to 23 ms |
| key to frame median, p95 | 21.6, 157 / 39.4, 99.5 / 22.1, 70.1 ms | 20.3, 41.4 / 17.5, 23.0 / 15.2, 20.7 ms |
| pointer to frame median | 21 to 39 ms | 16 to 19 ms |
| engine CPU, edit at rest | 92 / 58 / 138% | 160 / 182 / 149% |
| engine CPU, edit hidden | 10 / 11 / 11% | 9 / 11 / 11% |
| engine CPU, play hidden | 30 / 33 / 33% | 29 / 35 / 28% |

Visible, the engine now keeps the display's rate on this loaded machine, and its tail is a fifth
of what it was. Key to frame is within two refreshes (16.7 ms) in one run of three; the other two
are 17.5 and 20.3 ms, so the budget is met only when the load dips. Hidden, CPU is unchanged: the
threads are back at their default class and the latch only acts while ticks come. Visible CPU rises
with the frames rendered (about 1.0 to 1.25% per fps before, 1.2 to 1.5% after): more updates, a
little more each, likely the back-to-back update after an overrun.

In abyme.app (bevy-studio's measured launch, release engine at 1fa2829, arena, edit mode; that
window ticks at 60 Hz): 59 fps, 0 dropped ticks, GPU 1.46 ms, 2 skipped during the click, drag and
resize, input to frame median 34.9 ms and p95 36.2 ms over 31 samples (before: 39 fps of 120,
95 dropped, 33.4 and 170 ms). The rate holds and the tail is gone; the median, about two refreshes
at 60 Hz, and the two skips look like the viewer holding the shown slot until its next vsync, which
bevy-studio is looking at on abyme's side. Not yet measured: a 120 Hz window in abyme.app, and a
quiet machine.
