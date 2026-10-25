# session-bytes

What a `SessionBytes` call costs (change `session-bytes`), on a release engine under
`bevy-ts dev --headless`. One engine per run runs every arm of `project/game.ts`:

- `put64k`, `get64k`, `read64k`: one 64 KB value, 1,000 calls each, each timed (µs). `loop mean`
  is the whole loop over 1,000, for a timer coarser than a call.
- `put10k`, `read10k`, `get10k`: 10k keys `"id/<n>"` of 64 B, every key once in one frame (ms),
  10 frames each. `count10k` is 10k calls of `count()`, a numeric export with no argument: the
  call alone.
- `frame-empty`, `frame-10k`: `First` to `Last` with an empty store, then with the 10k keys stored
  and nothing called (ms).

Pass: `put64k` under 50 µs a call and `read10k` under 2 ms a frame.

```
cargo build --release -p bevy-ts-cli && cp target/release/bevy-ts target/session-bytes-bench
node bench/session-bytes/bench.mjs after target/session-bytes-bench
node bench/session-bytes/summary.mjs after
```

The `session-bytes-bench` process in `.abyme/processes.ts` does the same.

## Results

2026-10-25, M-series laptop under load (1-minute load 40-70: other worktrees building), 3 runs
each, medians. `glue-before` is the export glue as it was; `glue-after` adds commit "an exported
call allocates nothing for an ascii string or a Uint8Array" (`crates/bevy-ts-v8/js/core.js`).
Load fell between the two (about 65 to 40), so `count10k`, which the glue change barely touches,
is the control.

| arm | glue-before | glue-after |
|---|---|---|
| put64k (µs a call) | 2.21 | 1.54 |
| read64k (µs a call) | 1.75 | 1.17 |
| get64k (µs a call) | 7.46 | 6.38 |
| put10k (ms a frame) | 8.91 | 2.46 |
| read10k (ms a frame) | 8.79 | 2.04 |
| get10k (ms a frame) | 41.2 | 5.32 |
| count10k (ms a frame) | 1.05 | 0.57 |
| frame-empty (ms) | 4.51 | 1.93 |
| frame-10k (ms) | 5.15 | 2.04 |

`put64k` passes by 30 times. `read10k` sits on its 2 ms line (1.82 to 2.18 over the runs) at
load 40-60; over `count10k` it went from 8.4 times to 3.6. `frame-10k` against `frame-empty`:
10k stored keys cost a frame nothing measurable. Raw rows in `results/glue-*.jsonl`.
