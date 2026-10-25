# gizmos

What drawing gizmos from TS costs (change `script-gizmos`, design 11): 1,000 lines a frame from a
TS system on a headless release engine, against the same 1,000 `line` calls from a Rust system.

TS arms (`src/game.ts`): `none`; `keys`, 1,000 `Keys.pressed` calls, the floor of a call to an
exported param; `lines_hex`, 1,000 `line` calls with `"#f00"`; `lines_srgb`, the same with
`srgb(1, 0, 0)`; `lines_array`, the same with `[1, 0, 0]`; `strip_f32`, one `linestrip` of 1,000
points from a `Float32Array`. Each run is a fresh engine: 60 warm-up frames, then `main` (`First`
to `Last`) and `sys` (inside the drawing system) averaged over 300. An arm's cost is its median
`main` over `none`'s; `sys` is the calls' own cost, free of the frame's noise.

Rust (`rust/`): criterion, a `MinimalPlugins` + `AssetPlugin` + `GizmoPlugin` frame with no
drawing (`empty`) and with a system drawing 1,000 lines (`lines_1000`).

```
cargo build --release -p bevy-ts-cli && cp target/release/bevy-ts target/gizmos-bench
node bench/gizmos/bench.mjs after target/gizmos-bench
node bench/gizmos/summary.mjs after
cd bench/gizmos/rust && cargo bench
```

The `bench-gizmos` process in `.abyme/processes.ts` does all of it. Results: `results/`, and
`docs/gizmos.md` "Cost".
