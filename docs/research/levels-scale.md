# Levels at scale: load, reload, expansion and writes

Task 6.3 of `openspec/changes/levels`. Measured 2026-10-20 on the shared Apple M5 Max (other agents building), release, V8 15.0, Bevy 0.20, with `crates/bevy-ts-cli/tests/it/levels_bench.rs`:

```
cargo static test --release -p bevy-ts-cli --test it levels_bench:: -- --ignored --nocapture
```

The level is N `@enemies::Grunt { @hp: … }` instances, each with a `Transform` patch, one per entity separated by `--`. `Grunt` is a TS scene component whose `scene` makes a `Health` with `initialHp`/`max` and one keyed child, so each instance is two entities.

| instances | entities | load, ms | unchanged reconcile, ms | expand call, ms | write per edit, ms |
|---:|---:|---:|---:|---:|---:|
| 1,000 | 2,000 | 37 | 22 | 11 | 26 |
| 10,000 | 20,000 | 271 | 257 | 65 | 215 |

- **Load** is from the generation's reload to the level reconciled: the asset load, the parse, one expansion call, and the mount.
- **Unchanged reconcile** is `reconcile_level` on a file that did not change: the compose, the one expansion host call, and tsx's keyed diff, which writes nothing.
- **Expand call** is tsx's host call alone: JS runs every instance's `scene` and returns descriptions.
- **Write per edit** is one `set` of one lane through `write_level`: read the file, splice, re-parse and verify, write atomically, replace the asset and reconcile every level the file is in. It is about one reconcile, so a drag release on a 10,000-instance level answers in about 0.2 s.

What was fixed on the way. The first run took 193 ms per edit at 1,000 instances and 9.75 s at 10,000. The writer's verify step compared every named entity's head by scanning all of the file's tokens once per entry, which is quadratic. It now finds each entry's tokens by binary search: 26 ms and 215 ms. The same run showed Bevy's file watcher panicking its thread when the asset root is a symlinked path (macOS `/var` is `/private/var`). `bevy-ts dev` now canonicalises the asset root before giving it to `AssetPlugin`.

Where a 10,000-instance write goes (a re-run, 196 ms per edit):

| writer: splice, two parses, verify | asset parse | compose | expand host call | tsx resolve and diff |
|---:|---:|---:|---:|---:|
| 33 ms | 9 ms | 44 ms | 49 ms | 71 ms |

About 83% is the whole-level reconcile that follows the write, not the write itself. A drag writes once, on release, so coalescing drags would not change this. The write path needs to be incremental: reconcile only the touched name paths, and reuse the expansion of instances whose props did not change.

What is left, if 10,000-instance levels become a target:

- A write reconciles the whole level. It could reconcile only the edited entity, since the writer knows the touched name paths.
- Every reconcile re-expands every instance in one host call, which is 65 ms of the 257. Expansions whose component, props and generation are unchanged could be cached by content.

## The compose regression, 2026-10-27

`scene-expansion-cache.md` measured the compose row at 165 to 276 ms. That was real, not load. On a shared machine at load 25 to 160, wall times swung 4x between rounds of the same binary. So the bench now also prints the unchanged reconcile split in this thread's CPU time, a median of 7 calls each, which load barely stretches. `levels-bench-ab` alternates a saved older binary with this tree's.

The cause is a62596a79 (2026-10-23, missing code loads as placeholders). Levels moved from `bevy_ts_tsx::expand` to the new `expand_each`, which normalizes each item's source paths with a new `desc::Files`. `Files::new` reads the working directory, and on macOS `getcwd` opens and stats folders. That is 10,000 directory walks per compose at 10,000 instances, about three quarters of the compose in a profile. The bench timed `expand`, not `expand_each`, so the cost showed up in the compose leftover. `expand_each` now makes one `Files` per call, and the bench times `expand_each`, the call levels make.

CPU ms at 10,000 instances (20,000 entities), medians over rounds, alternated:

| tree | rounds | reconcile | compose | expand | tsx | after the diff |
|---|---:|---:|---:|---:|---:|---:|
| 418fd701d, where the 44 ms was measured | 3 | 305 | 71 | 86 | 144 | 4 |
| HEAD before the fix | 3 | 576 | 379 | 85 | 146 | -34 |
| 418fd701d | 11 | 260 | 55 | 77 | 111 | 2 |
| HEAD with the fix | 11 | 310 | 84 | 75 | 114 | 33 |

What is left: compose is about 28 ms above 418fd701d, and the stamps after the diff about 31 ms. A second profile showed no hot spot, so this is the work added since (placeholders, missing components, scene placements). The wall-clock 44 ms was a quiet-machine number; the same commit reads 55 to 71 ms of CPU at this load.
