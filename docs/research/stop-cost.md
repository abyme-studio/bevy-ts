# What a Stop costs

Task 6 of `openspec/changes/play-stop`. A Stop rebuilds the edit world from source: a fresh
isolate, the live generation applied again, and every open level remounted. Measured 2026-10-23 on
the shared Apple M5 Max with other agents building (load average 22 to 60), release, V8 15.0,
Bevy 0.20, with `crates/bevy-ts-cli/tests/it/stop_bench.rs`:

```
cargo static test --release -p bevy-ts-cli --test it stop_bench:: -- --ignored --nocapture
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

## The restore (stop-incremental)

Task 6 of `openspec/changes/stop-incremental`. A level the game started with now keeps its
entities at Stop; only what the play changed respawns, from the level's kept resolved tree.
Measured 2026-10-23 with the same bench, now in four arms: the full remount (the level made to
wait to compose, so it is not kept), and the restore with nothing touched, 100 enemies moved,
and every enemy moved. Three Stops per arm, medians, rendering on (`DefaultPlugins` without a
window, the editor camera swapped for the game's at Play). Load average 26 to 38.

| instances | entities | arm | Stop, ms | plan, ms | rebuild, ms | restore, ms | settle, ms |
|---:|---:|---|---:|---:|---:|---:|---:|
| 1,000 | 2,000 | full remount | 224 | 0 | 43 | 0 | 161 |
| 1,000 | 2,000 | none touched | 51 | 1.0 | 37 | 0.2 | 0 |
| 1,000 | 2,000 | 100 touched | 67 | 0.9 | 42 | 2.4 | 0 |
| 1,000 | 2,000 | all touched | 70 | 0.6 | 86 | 12 | 0 |
| 10,000 | 20,000 | full remount | 1,652 | 0 | 226 | 0 | 1,094 |
| 10,000 | 20,000 | none touched | 161 | 8.3 | 108 | 2.7 | 0 |
| 10,000 | 20,000 | 100 touched | 152 | 6.0 | 84 | 9.3 | 0 |
| 10,000 | 20,000 | all touched | 201 | 3.7 | 73 | 114 | 0 |

The three runs of each 10,000 arm: none touched 109, 161, 204 ms; 100 touched 118, 152, 152;
all touched 201, 213, 201; the full remount 1,480 to 1,852.

A second round on the rebased branch, at load 39 to 65 (every team landing at once), medians:

| instances | arm | Stop, ms | plan, ms | rebuild, ms | restore, ms |
|---:|---|---:|---:|---:|---:|
| 1,000 | full remount | 167 | 0 | 48 | 0 |
| 1,000 | none touched | 50 | 0.7 | 35 | 0.3 |
| 1,000 | 100 touched | 38 | 0.6 | 35 | 2.5 |
| 1,000 | all touched | 46 | 0.5 | 34 | 13 |
| 10,000 | full remount | 1,395 | 0 | 155 | 0 |
| 10,000 | none touched | 107 | 12 | 144 | 3.3 |
| 10,000 | 100 touched | 279 | 8.1 | 147 | 14 |
| 10,000 | all touched | 231 | 5.2 | 87 | 159 |

The 10,000 runs: none touched 106, 106, 110 ms; 100 touched 243, 279, 285; all touched 231,
232, 298; the full remount 1,158 to 1,457. Neither round had a quiet machine: the rebuild alone
moved between 73 and 150 ms with the load.

A third round after the gate, at load 18 to 35, medians:

| instances | arm | Stop, ms | plan, ms | rebuild, ms | restore, ms |
|---:|---|---:|---:|---:|---:|
| 1,000 | full remount | 158 | 0 | 50 | 0 |
| 1,000 | none touched | 40 | 1.1 | 68 | 0.3 |
| 1,000 | 100 touched | 43 | 1.0 | 41 | 3.0 |
| 1,000 | all touched | 53 | 0.5 | 35 | 13 |
| 10,000 | full remount | 841 | 0 | 159 | 0 |
| 10,000 | none touched | 98 | 7.1 | 113 | 3.5 |
| 10,000 | 100 touched | 131 | 7.6 | 98 | 10 |
| 10,000 | all touched | 207 | 4.1 | 87 | 142 |

The 10,000 runs: none touched 98, 98, 111 ms; 100 touched 127, 131, 143; all touched 192, 207,
248; the full remount 803 to 848.

- **Plan** is `levels::plan_stop`: the scan of the level's 20,000 entities against the game-start
  baseline, and the level file read and hashed.
- **Restore** is `levels::restore`: `apply_at` respawning the changed subtrees, and their stamps.
- **Rebuild** keeps the level's entities, so it despawns nothing of it, but still loads the
  generation in a fresh isolate and checks every level against it (`levels`' section check
  prepares the whole level). It grows with the level (37 ms at 2,000 entities, 108 at 20,000);
  which part grows is not split yet, and the level check is the likely one.

The kept resolved tree costs 9.0 MB at 20,000 entities (0.9 MB at 2,000), its components shared
with the template cache.

With rendering on, `ViewVisibility` changed on every level entity under Play's camera swap; a
level camera's `VisibleEntities`, `Clusters` and `PreviousViewData` change every frame. These are
tsx's `restore::derived` list and do not count; with it, a play that touches nothing respawns 0
entities. `Camera` changes every frame too but counts, so a level camera respawns.

The restore takes a 10,000-instance Stop from 0.8 to 1.9 s to 0.1 to 0.3 s. The 100 ms goal is met
only with nothing touched at the quietest (98 ms): the rebuild's 73 to 150 ms is most of every arm. The next cut is in the rebuild: split it,
and if the level check is the part that grows, skip it for a Stop, which does not need it (the same module set, the level files hashed as unchanged), and
the content-keyed expansion cache for the full path.
