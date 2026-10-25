# diagnostics

What each Bevy diagnostics plugin costs a frame (change `script-diagnostics`): n entities (each a
`Transform` and a TS `Thing`, 1,000 of them drawn as cubes), headless, with no plugin and then each
plugin alone in `bevy-ts.json`'s `diagnostics`. `main` is the time from `First` to `Last` (the
main world's frame), `frame` the time between `Last`s; under `--headless` the frame is paced, so
the plugin's cost is in `main`.

```
cargo build --release -p bevy-ts-cli && cp target/release/bevy-ts target/diagnostics-bench
node bench/diagnostics/bench.mjs after target/diagnostics-bench
node bench/diagnostics/summary.mjs after
```

The `diagnostics-bench` process in `.abyme/processes.ts` does the same.
