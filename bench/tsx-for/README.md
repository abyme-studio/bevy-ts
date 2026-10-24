# tsx-for

What one change to a keyed `<For>` costs (change `tsx-keyed-for-linear`). One list of n rows under
one parent; arms, each at 1k, 10k and 100k:

- `grow`: 0 to n items in one signal set;
- `grow1k`: 1,000 more items a frame until n;
- `reverse`: n rows mounted, then reversed in one set;
- `clear`: n rows mounted, then emptied in one set;
- `spawn`: the same n rows with `commands.spawn`, in one frame: the reference.

The time is from the set (in `Update`) to the end of the frame (`Last`) the rows are all there (gone,
for `clear`; the set's own frame for `reverse`). Each run is a fresh headless engine.

```
cargo build --release -p bevy-ts-cli && cp target/release/bevy-ts target/tsx-for-after
node bench/tsx-for/bench.mjs after target/tsx-for-after
node bench/tsx-for/summary.mjs before after
```

The `tsx-for-before` and `tsx-for-after` processes in `.abyme/processes.ts` do the same.
