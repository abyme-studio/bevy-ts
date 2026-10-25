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
