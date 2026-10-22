# An incremental Stop: where 20,000 level entities go, and how to get under 100 ms

Research for `stop-incremental`. Measured 2026-10-23 on the shared Apple M5 Max, load average 12
to 35 (other agents building), release, V8 15.0, Bevy 0.20. The level, game and app are
`stop_bench.rs`'s (`docs/research/stop-cost.md`): N `@enemies::Grunt` instances, two entities
each. The bench was play-snapshot's `stop_bench.rs` with timers around each step of `stop`, in a
scratch worktree off `play-snapshot` (not committed). Three processes, each three Stops per arm;
the tables give the median of the three medians, with the spread.

## 1. Where today's Stop goes

Arm A is today's Stop after `OnExit`: `generation::rebuild`, `restore_open`, `settle`.

| step | 1,000 instances, ms | 10,000 instances, ms | 10,000 spread |
|---|---:|---:|---|
| rebuild: tsx's restart handler despawns every `TsxNode` | 2.4 | 26 | 26 to 27 |
| rebuild: isolate restart | 8.4 | 8.6 | 7.7 to 8.8 |
| rebuild: load and apply the live set | 10.6 | 39 | 38 to 45 |
| levels: read, parse, prepare (compose) | 3.4 | 33 | 30 to 34 |
| levels: expand host call (`expand_each`) | 21.7 | 171 | 166 to 195 |
| levels: finish compose, placeholders | 1.8 | 17 | 16 to 19 |
| tsx: resolve | 5.1 | 48 | 46 to 57 |
| tsx: diff and apply (spawn, insert) | 3.1 | 31 | 31 to 33 |
| levels: stamp `LevelInstance`, `MissingComponents` | 2.0 | **252** | 249 to 296 |
| **total** | 61 | **649** | 633 to 741 |

The rest of the total (about 25 ms at 10,000) is `restore_open` and `apply_composed`'s
bookkeeping. stop-cost.md's 1,036 ms was the same work at load 22 to 60.

Two findings that are bugs, not design:

- **Stamping is quadratic.** 2.0 ms at 1,000, 252 ms at 10,000. `stamp_instances` calls
  `writer::written` for every instance whose `LevelInstance` changed, and on a fresh spawn that is
  all of them. `written` walks the parsed file with `find_in`, a linear scan of the siblings by
  name. 10,000 instances at the top level is 50 million name compares. It came with
  `374bf42 feat(levels): edit levels from the studio` (2026-10-22), after levels-scale.md measured
  a 271 ms load. It also slows every level open and every reconcile that respawns. Fix: index the
  file's names once per stamp. Expected saving: about 245 ms at 10,000.
- **Expansion is about 3 times slower than levels-scale.md measured.** 171 ms against 49 to 65 ms.
  Under `bevy-ts dev` levels expand through `expand_each` (each item answers for itself), which
  came with the placeholder work on 2026-10-23. A warm isolate (arm B) is no faster: 149 ms. The
  machine was busier then, so the factor is likely, not sure. Worth a profile of `expand_call`'s
  per-item error path on its own.

## 2. A content-keyed expansion cache

On a Stop every expansion would hit: the same files, the same module set. It saves the expand
call: **171 ms at 10,000**, 22 at 1,000 (149 ms warm).

What the key must cover, per instance:

- **The scene component's name and its props**, as composed (canonical JSON).
- **The live module set's content hash**, not the generation number: a Stop applies the same set
  as generation N+1. A hash of the whole set covers imports, helpers and other modules a
  `scene()` reads.
- **`at`, the instance's place in the `.bsn` file.** The description's source chain carries
  the call site, which the editor shows and placeholders point at. Either key on it or rewrite the
  outer frame on a hit; keying is simpler and a Stop never moves lines.
- **Dev or ship (`tolerant`)**: `expand_each` and `expand` answer differently.
- **An error is cached as the answer.** A throwing `scene()` throws again for the same code and
  props.

Not in the key, because they run after expansion: patches (`Prepared::finish`), placeholders and
`MissingComponents` (`resolve_missing`, which also reads the build-failure state), and native
layouts (fixed per binary).

What it assumes: **`scene()` is a function of its props.** One reading `Math.random`, `Date` or
a TS resource would change on a cache hit. That must be the documented rule. Bevy's own
`Template::build` has world access, so Bevy does not promise this. Expansions that carry bound
sites (`{"$b", "$s"}`) hold closures in the isolate. A Stop restarts the isolate, so those
instances must expand again: never cache them.

## 3. Keeping untouched entities

**Identity is already stable.** A tsx node's id is `node_id(root, key_path)`: the level root and
its name path, plus `key`s inside a prefab. Hot reloads keep entities this way today. A Stop loses
them only because `restart: true` runs tsx's restart handler: every `TsxNode` despawned, the
`TsxTree` reset (the tsx-hot-reload spec: "every scene mounts as if first").

Arm B keeps the tree and settles the level as today: compose, expand, resolve, diff. Nothing is
written.

| step | 10,000 instances, ms | spread |
|---|---:|---|
| read, parse, prepare | 31 | 30 to 36 |
| expand host call | 149 | 138 to 156 |
| finish compose | 17 | 16 to 19 |
| tsx: resolve | 17 | 17 to 19 |
| tsx: diff (writes nothing) | 59 | 57 to 65 |
| stamp (nothing changed) | 9 | 9 to 12 |
| **settle total** | **292** | 283 to 328 |

Keeping the tree saves the despawn (26), the spawn (31), 31 ms of resolve (the template cache
hits) and the quadratic stamp. But a full recompose and diff of 20,000 entities still costs 292 ms.
1,000 instances: 34 ms.

**Which entities the play touched: Bevy's change ticks.** Arm C records the tick and each level
entity's archetype at Play, writes 100 `Transform`s, then checks every component's change tick
and the archetype. All 100 are found. **0.32 ms at 10,000 instances** (0.29 to 0.61), 0.02 at
1,000. TS writes mark Bevy's per-row tick (`systems.rs` hands JS the tick column), so TS and Rust
writes are both seen. A different archetype catches components added or removed.

What a kept entity breaks, and the answer:

| what | today (respawn) | kept | answer |
|---|---|---|---|
| TS component values | fresh | survive the isolate restart | Plain bytes in Bevy columns. The same layout reuses its `ComponentId` (`Planned::Reuse`). |
| Entities the game despawned | respawned (tree reset) | tombstoned: not respawned | Tombstones made during the play are cleared and respawned at Stop. This changes the tsx-hot-reload spec. |
| Components the game added or removed | gone with the entity | archetype differs | Touched: the subtree is restored. |
| Game-spawned children (`TsSpawned`) | despawned by rebuild | despawned by rebuild | `Children` on the kept parent changes. Scan before the despawn, or treat relationship targets as derived. Restoring too much is correct, only slower. |
| `Added`/`Changed` in TS systems | all added | all added | The rebuild makes fresh systems, so the first run sees everything. |
| `Added`/`Changed` in Rust systems | added again | not added | Their `last_run` outlives a Stop. Rapier's entities are touched in play (handles inserted, bodies moved), so they are restored anyway. A plugin that does nothing to an entity on `Added` and keeps state across Stop sees no new `Added`. That is Bevy's own meaning ("nothing changed"). Document it. |
| Hooks and observers | `on_remove`/`on_despawn` then `on_add`/`on_insert` fire | none fire | State kept in step by add/remove pairs stays in step either way. TS observers are queued and cleared at `enter_play`, so they see nothing in either case. |
| `MissingComponents`, placeholders | stamped again | kept | The compose did not change. A change shows up as a touch. |
| Bound sites in a description | registered again by the new isolate | dead closures | Those instances expand again (section 2). |
| Selection, entity ids | entities change, followed by node | unchanged | Better. |
| Level file changed during the play | recomposed | hash differs | Full reconcile of that level, the arm B path. |
| Generation hot-reloaded during the play | recomposed | module-set hash differs | Full reconcile, cache misses. |

## 4. Do the two reach 100 ms at 20,000 entities?

Not as levels-scale.md framed them. A kept tree that still recomposes and diffs, with every
expansion cached:

| path at 10,000 instances | ms |
|---|---:|
| today, as measured | 649 |
| today, stamp made linear | ~400 |
| kept tree, full recompose and diff (arm B) + rebuild without despawn (48) | ~340 |
| the same with every expansion cached | ~190 |
| **Stop as a restore** (below), nothing touched | **~50** |
| Stop as a restore, 100 entities touched | ~50 |
| Stop as a restore, every entity touched | ~110 |

**What else is needed: Stop restores, it does not reconcile.** If no level file changed and the
module set is the one the levels were composed with, the level description is the one already
applied. The diff can only find what the play changed, and the ticks name that directly. So:

1. At Play: record the change tick, each level entity's archetype, and the tombstone set. Keep
   each level root's last resolved tree (an `Arc`; tsx already resolves into it).
2. At Stop: scan the level entities' ticks and archetypes before the rebuild despawns
   `TsSpawned` (0.3 ms). The rebuild skips despawning level roots' nodes. Then restore only the
   touched subtrees, and the nodes tombstoned during the play, from the kept resolved tree. tsx
   needs one new primitive: apply a root's resolved tree at given key paths only. levels-scale.md
   asked for the same primitive so that a write reconciles only the paths it touched, so one
   foundation serves both.
3. If a file hash or the module-set hash differs, or an instance carries bound sites, that level
   (or instance) takes the full path, with the expansion cache.

Cost at 10,000: rebuild without the despawn (isolate 9 + load and apply 39 = 48) + scan (0.3) +
file hash check (one read and FNV of a 600 KB file, about 1 ms) + restoring k subtrees at about 2 µs an entity (resolve is skipped; spawn and insert are 1.5 µs; only the restored ones are stamped).
Untouched or lightly touched: **about 50 ms**. A play that touches every level entity, say physics
on everything: despawn 26 + spawn 31 + stamp ~10 + rebuild 48, **about 115 ms**. That is just
over the target. Writing the description's values back in place, instead of respawning, would bring
it under; measure before building.

The rebuild's 39 ms of load and apply is then the largest part. It scales with the game's code,
not its level.

## 5. Precedent

- **Bevy 0.20 BSN spawns; it does not reconcile.** `ScenePatch` resolves a scene once and caches
  the `ResolvedSceneRoot` on the asset (`bevy_scene/src/scene_patch.rs`), invalidated when the
  asset changes, and spawns from it many times. That is the precedent for section 2 and for
  keeping the resolved tree: cache resolution by content, key it on the asset. Bevy ships no
  `.bsn` asset format, no scene hot reload that keeps entities, and nothing like a play baseline.
- **Bevy's change ticks are its "did anything touch this".** `Changed<T>` reads the same
  per-row tick the scan reads. Using it at Stop invents no tracking.
- **Other editors snapshot or isolate; none reconciles.** Unity leaves play mode by restoring the
  scene it saved on entering. Unreal's Play In Editor runs a duplicated world and throws it away.
  Godot runs the game as a separate process. play-stop's design rejects a snapshot (V8's heap
  can't be kept, and Bevy's cloner skips TS components). The restore above is the closest
  equivalent: it puts back only what changed, from the description it was built from.

Sources: `bevy_scene-0.20.0` source;
[Unity, configurable Enter Play Mode](https://docs.unity.com/en-us/engine/6000.7/manual/programming-environment/code-reload-serialization/code-reload/code-reloading-editor/configurable-enter-play-mode.md);
[Bevy 0.20 release notes](https://bevy.org/news/bevy-0-20/).

## Recommendation

1. **Now, as its own fix:** make `stamp_instances`' `written` lookup linear. About 245 ms off every
   10,000-instance open, reload and Stop. Small. Fix it before anything else.
2. **The change: Stop as a restore** (section 4), on Bevy's change ticks and tsx's existing node
   ids, with tsx's "apply at key paths" primitive. Expected: about 50 ms at 20,000 entities when
   the play touched few of them, about 115 ms when it touched all. It changes one spec line:
   tsx-hot-reload's "restart clears every tombstone and the retained tree" becomes "a Stop restores
   the tree as it was at Play".
3. **Then, for edits and generation changes:** the content-keyed expansion cache (section 2). It
   saves 150 to 170 ms whenever a level recomposes with code that did not change. That is
   levels-scale.md's write path, not a Stop's once step 2 lands.
4. **Separately:** profile `expand_each`, which is about 3 times slower than the expansion
   levels-scale.md measured.

For the operator: no new dependency or infrastructure. Step 2 is a new mechanism, but it extends
the reconciler that exists: a Play baseline (tick, archetypes, tombstones) and a resolved tree kept
in memory per level root, a few MB at 20,000 entities. Step 3 makes `scene()` purity a rule
authors must follow.
