## 0. Before

- [x] 0.1 play-stop has landed on main and this branch is rebased on it; the `stamp_instances` fix has landed

## 1. Start notification (`crates/bevy-ts`)

- [x] 1.1 `plugin_api.rs`: `on_ts_start`, the shape of `on_ts_restart`
- [x] 1.2 `generation::start_game` notifies once, before `run_startups`; a game booted in play notifies at its first start
- [x] 1.3 Tests: Play then step notifies once; Play, Stop, Play notifies twice

## 2. tsx (`crates/bevy-ts-tsx`)

- [x] 2.1 `RootState.resolved`: the kept `Arc` resolved tree of an external root, set by `apply_root`, dropped on unmount and restart
- [x] 2.2 `apply_at(world, root, items, mode)` with `Respawn` and `Reconcile`, sharing `apply_root`'s per-node code; placement in description order; tombstones cleared on respawn; `TsxTree`, the kept tree and the watch journal updated
- [x] 2.3 The baseline on `on_ts_start`: tick, archetype per node, apply count, per applied external root; dropped on restart and Stop
- [x] 2.4 `changed_paths(world, root)`: the scan (archetype, added and changed ticks), reduced to the highest changed node per subtree; what Bevy derives every frame (`restore::derived`) does not count
- [x] 2.5 `has_callbacks(world, root)`: any binding, handler, ref or slot under the root
- [x] 2.6 `StopPlan` resource; the restart handler keeps the plan's roots (nodes, `RootState`, entities, kept tree) and moves their tombstones into the plan
- [x] 2.7 Unit tests: `apply_at` respawn of one enemy (siblings keep entities and ticks), reconcile of one field, a tombstoned node; the scan finds a TS write, a Rust write, an added and a removed component, and nothing when nothing changed

## 3. levels (`crates/bevy-ts-levels`)

- [x] 3.1 `plan_stop(world) -> StopPlan`: kept roots (baseline, same apply count, not dirty, file hashes equal to `applied`, not a sidecar, no callbacks), each with its changed and tombstoned paths
- [x] 3.2 `after_restart` and `after_generation` leave the plan's roots clean
- [x] 3.3 `restore(world, &plan)`: `apply_at(..., Respawn)` per kept root, then stamp only the respawned nodes; `level+watch` reports them

## 4. Stop (`crates/bevy-ts-viewport`)

- [x] 4.1 `stop()`: `plan_stop` first, then play-stop's steps with `levels::restore` between `restore_open` and `settle`; the plan removed after

## 5. Tests

- [x] 5.1 `crates/bevy-ts-cli/tests/play_stop.rs` (levels with scene components need V8 and the compiler; the start notification's in `crates/bevy-ts-viewport/tests/edit.rs`), one per edit-mode scenario: nothing touched (entity ids kept, watch empty), one enemy moved and one given `Stunned`, a killed enemy, a stepped game, a level saved during play, a level with a handler, the selection keeps its entity, a native `Added` counter
- [x] 5.2 play-stop's Stop tests pass unchanged (the arena, paused, a save during play, the camera, time, levels and selection)
- [x] 5.3 tsx-hot-reload: a Stop keeps `b` and respawns `a`; `restart` still mounts as if first
- [x] 5.4 `examples/rapier` under `--edit`: play, Stop, play gives the same body positions frame for frame (play-stop's check, now with kept entities)

## 6. Measure

- [ ] 6.1 `crates/bevy-ts-cli/tests/stop_bench.rs` (`--ignored`, release) at 1k and 10k instances (2k and 20k entities), 3 runs per arm: play-stop's full Stop; restore with nothing touched; 100 touched; every entity touched. Steps timed: plan (scan, hashes), rebuild, restore, settle
- [x] 6.2 The kept trees' memory at 20k entities
- [x] 6.3 The same bench with rendering on (in a tart VM or headless render): does any derived component mark every entity?
- [ ] 6.4 `docs/research/stop-cost.md`: the table; targets about 50 ms nothing touched and about 115 ms all touched at 20k

## 7. Docs

- [x] 7.1 `docs/viewport.md`: what Stop keeps and respawns, when a level takes the full path, entity ids kept
- [x] 7.2 `docs/native.md`: no `Added`, hooks or observers for kept entities after a Stop; reset in `OnExit(EngineMode::Play)`
- [ ] 7.3 Workspace gate once, last: build, tests (viewport's render tests one at a time), clippy
