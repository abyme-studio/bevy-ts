# What a Stop costs

Task 6 of `openspec/changes/play-stop`. A Stop rebuilds the edit world from source: a fresh
isolate, the live generation applied again, and every open level remounted. Measured 2026-10-23 on
the shared Apple M5 Max with other agents building (load average 22 to 60), release, V8 15.0,
Bevy 0.20, with `crates/bevy-ts-cli/tests/stop_bench.rs`:

```
cargo test --release -p bevy-ts-cli --test stop_bench -- --ignored --nocapture
```

The level is the levels bench's (`docs/research/levels-scale.md`): N `@enemies::Grunt` instances,
two entities each, opened by `app.level`. The game has one `Startup` system that spawns. Each size
runs three Stops end to end through `viewport_mode`, then three with their parts timed one by one,
in the order Stop calls them. Medians of the quietest of four runs (the `stop-bench-ab` process ran
the levels bench and this one back to back, three times, so both saw one load):

| instances | entities | Stop, ms | `OnExit(Play)`, ms | rebuild, ms | levels restore and settle, ms | next Play with `Startup`, ms |
|---:|---:|---:|---:|---:|---:|---:|
| 1,000 | 2,000 | 81 | 0.1 | 25 | 54 | 0.5 |
| 10,000 | 20,000 | 1,124 | 0.2 | 134 | 1,036 | 0.6 |

- **Stop** is the `viewport_mode { mode: "edit" }` call from play: `OnExit`, the camera swap, the
  rebuild, the levels, the clocks and the selection, until it answers.
- **Rebuild** is `generation::rebuild`: TS entities and TSX nodes despawned, a fresh isolate, and
  the live module set loaded and applied, with no compile.
- **Levels restore and settle** is putting back the levels open at Play and composing each from
  disk: the parse, one expansion host call, and the mount of every entity.
- **Next Play** is the following `viewport_mode { mode: "play" }`, which runs `Startup`.

The busier runs gave 132 to 191 ms at 1,000 and 1.2 to 1.9 s at 10,000; the parts kept their shares.

## Where it goes

The level is almost all of it. The levels bench, in the same round, took 1,019 ms to load the
10,000-instance level and 116 ms the 1,000 one (1,934 and 191 ms in the busiest round; 271 and
37 ms on 2026-10-20 on a quieter machine). A Stop is about one level load plus the rebuild: the
remount is the same work as opening the level.
`OnExit` and the next Play's `Startup` are under a millisecond.

At 1,000 instances a Stop is well under the Play button's feel. At 10,000 it is over the 1 s the
design set as the line.

## Follow-up

At 10,000 instances, not a snapshot (the design rejects one: V8's heap can't be kept, and Bevy's
cloner skips TS components). The follow-up is the incremental reconcile `levels-scale.md` already
names:

- **Cache expansions by content.** An instance whose component, props and generation are
  unchanged reuses its description. A Stop re-applies the same generation, so every expansion
  would hit.
- **Keep level entities across a Stop when the game did not touch them.** A level entity whose
  components the play did not change, and whose file did not change, need not despawn and mount
  again. This needs change ticks on level entities and is the larger piece.

Neither is in this change: no game today has a 10,000-instance level.
