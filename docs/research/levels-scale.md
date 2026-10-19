# Levels at scale: load, reload, expansion and writes

Task 6.3 of `openspec/changes/levels`. Measured 2026-10-20 on the shared Apple M5 Max (other agents building), release, V8 15.0, Bevy 0.20, with `crates/bevy-ts-cli/tests/levels_bench.rs`:

```
cargo test --release -p bevy-ts-cli --test levels_bench -- --ignored --nocapture
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
