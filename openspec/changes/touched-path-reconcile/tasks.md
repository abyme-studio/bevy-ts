## 0. Before

- [x] 0.1 compose-regression has landed on main and this branch is rebased on it; check `compose.rs` still has `prepare`/`finish` and `build_list`, else adapt decision 4 to its structure first. compose-regression never landed (no branch, no commit on main, 2026-11-05); `compose.rs` has `prepare`/`finish` and `build_list` as decision 4 assumes. The full path keeps today's compose cost
- [x] 0.2 Bench first (6.1, 6.2): record today's split, before any change (bench/edit-split/results/today.txt, 3 runs at load 70-90: 0.2-1.0 s per write at 2,000 entities, 2.7-6.7 s at 20,000; step split comes with EditSteps)

## 1. Writer (`crates/bevy-ts-levels/src/writer.rs`)

- [x] 1.1 `Written.units`: each op's touched name paths and whether removed (design, decision 1), positional renames included, reduced to the highest
- [x] 1.2 `Written.parsed`: the verify step's parse of the new text
- [x] 1.3 Unit tests in `writer.rs`: units of each op kind, a reparent's two, a rename's shifted siblings, units under one another reduced
- [x] 1.4 (done as `Written.moves`, line and column anchors per op; 1.5 made the full-path fallback unneeded) `Written.shifts`: per op, the line and delta where a splice changes the line count with content after it. Any shift takes the full path at first (sources, owners, kept-tree locations below the edit would be stale)
- [x] 1.5 Required (lead): shift lines in place instead (`TsxNode.source`, component sources and overrides, `LevelInstance` owner lines, `LevelRoot` owners and diagnostics, kept-tree locations), so a delete or reparent stays partial

## 2. Compose (`crates/bevy-ts-levels/src/compose.rs`)

- [x] 2.1 `prepare_at(path, files, schema, tolerant, units)`: a key-path trie in `Env`; `build_list` builds only entries on a unit's path, everything at and under one; `prepare` is `prepare_at` with no filter
- [x] 2.2 The unit rises to the first scene component instance on its path; an include does not stop it
- [x] 2.3 `Composed` for a filtered compose: `(key path, Option<NodeDesc>)` per unit; owners, scenes, missing for the units only
- [x] 2.4 Warnings and asset paths by key path (file-level ones under the empty path)
- [x] 2.5 Unit tests with the fake `Expander`: one of many instances (one request), a path under an include with an outer override, a removed path, an inner prefab path rising to its instance; each subtree equal to the whole compose's

## 3. tsx (`crates/bevy-ts-tsx`)

- [x] 3.1 `reconcile.rs`: `replace_node` places a new node at its described index, not last
- [x] 3.2 `lib.rs` `rekey`: moves the kept resolved tree's node to the new path
- [x] 3.3 `reconcile.rs` or `restore.rs`: a subtree callback check (binding, handler, ref, slot) for a resolved subtree and for the kept one at a path; `has_callbacks` uses it
- [x] 3.4 `lib.rs`: `reconcile_external_at(world, root, items)`: resolve each unit with its parent's path and types as prefix, `apply_at(..., Reconcile)`, journal closed
- [x] 3.5 (in `tests/it/restore.rs`, beside the kept-tree helpers) Tests in `crates/bevy-ts-tsx/tests/it/reconcile.rs`: a new node between two (world and kept tree order), a rekey then an equal apply writes nothing, a removed node despawns and siblings keep ticks

## 4. Levels (`crates/bevy-ts-levels/src/engine.rs`, `edit.rs`)

- [x] 4.1 `LevelRoot.warnings` and `assets` by key path; `loaded` lists diagnostics in key path order
- [x] 4.2 `engine::reconcile_at(world, level, units)`: compose at the units, `reconcile_external_at`, `stamp_instances` and `stamp_missing` with `only`, patch `LevelRoot` per unit, `applied` hash, emit `loaded`; any error falls back to `reconcile_level`
- [x] 4.3 `edit.rs` `write_inner`: map units through `placements` per root; the full-path checks (decision 7); else `reconcile_at`
- [x] 4.4 `load_text` takes `Written.parsed`
- [x] 4.5 `PartialEdits(bool)` resource, on by default
- [x] 4.6 `EditSteps` resource: ms per step, full or partial and why, instances expanded, nodes checked; written by `write_level`

## 5. Tests

- [x] 5.1 `crates/bevy-ts-levels/tests/it/levels.rs` (native components, fake backend): a set, an add at an index, a delete, a reparent, a rename, a component add and remove, each checking `EditSteps` partial and the untouched entities' ids and change ticks
- [x] 5.2 Same file: the equivalence test. Each op kind on the same files with `PartialEdits` on and off; `tsx_tree`, components and `LevelInstance`s equal
- [x] 5.3 Same file: the full-path cases: an include added, a single top changed, a level that failed its last reconcile, a rebased write (disk change whole, the write partial)
- [x] 5.4 (the callback case is a binding, `max={() => hp}`) `crates/bevy-ts-cli/tests/it/levels_scene.rs` (V8): one of many `Grunt`s edited expands one instance; an inner `eye` override reconciles its instance; a scene with `onClick` takes the full path
- [x] 5.5 ~~Existing levels, levels_protocol, levels_walk, play_stop and dev_edit tests pass unchanged~~ Dropped (operator, 2026-11-05: tests opt-in; only 5.1-5.4 run)

## 6. Bench (`bench/edit-split/`)

- [x] 6.1 New crate outside the workspace, as `bench/world-saves`: `Cargo.toml` with path deps on `bevy-ts`, `bevy-ts-v8`, `bevy-ts-tsx`, `bevy-ts-levels`, `bevy-ts-cli`; `run.sh [runs]`; `src/main.rs` builds `levels_bench`'s app and level
- [x] 6.2 At 1,000 and 10,000 instances (2,000 and 20,000 entities), 3 runs, medians: for a move, a prop edit, an add, a delete and a rename, `PartialEdits` off then on; print `EditSteps` per step (compose, expand, reconcile, other = total less those) as a markdown table
- [x] 6.3 `docs/research/touched-path-reconcile.md`: the table, today against the plan, with the targets: under 50 ms per write at 20,000 entities and under 8 at 2,000; compose, expand and reconcile each under 2 ms at 20,000

## 7. Docs

- [x] 7.1 `docs/levels.md`: a write reconciles what it touched; when it reconciles the whole level; external changes still whole
- [x] 7.2 ~~Workspace gate once, last: build, tests, clippy~~ Dropped (operator, 2026-11-05: checks cut; land's check runs on the batch)
