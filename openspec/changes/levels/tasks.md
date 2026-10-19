## 1. Format and parser (no engine)

- [x] 1.1 Spike: E2 is Bevy 0.20's `@SceneComp { @prop: v, field: v }` (settled from `bevy_scene/src/lib.rs`); check whether `#26043`'s lexer accepts comments; record both in design.md
- [x] 1.2 Vendor `bevy_bsn`'s syntax module at the #26043 commit under `crates/bevy-ts-levels/src/upstream/`, with its licence, for tests only
- [x] 1.3 Write the lossless lexer and parser (tokens with trivia, byte spans) for upstream's grammar plus E1 to E4; reject every other construct with a named error and location
- [x] 1.4 Printer and round-trip tests: a fixture corpus (comments, blank lines, one-line and multi-line patches, trailing commas) prints back byte for byte
- [x] 1.5 Differential test: every extension-free fixture parses to the same tree with the vendored parser; `bevy-ts levels check --upstream` lists extensions with locations

## 2. Loading levels

- [ ] 2.1 Confirm tsx T1 (scene components by name with fields and props read by the compile step, and a host entry) and T2 (external roots); add `app.level()` to the TS package with types
- [ ] 2.2 `BsnLevel` asset loader (`bsn` extension) and tree-to-description conversion: names as keys, patches as controlled component elements, `Children` lists
- [ ] 2.3 Value typing: Rust components through the type registry with short-path resolution and ambiguity errors, TS components through layouts; errors with file, line, column
- [ ] 2.4 Asset paths in `Handle` fields: asset-root and `./`/`../` relative resolution, `ReflectConvert` where 0.20 has it (verify) and own conversion otherwise; missing-asset warnings
- [ ] 2.5 `@Comp` expansion through the TS scene component's `scene` in one host call per reconcile, scene component added to the root, `field:` patches applied to it; Transform and other root patches layered after the expansion
- [ ] 2.6 E3 merge by name into expansion children (keys and quoted `tsx` segments), new children otherwise; reject removal of prefab-internal entities
- [ ] 2.7 `:"x.bsn"` inclusion with overrides through any depth and a cycle check
- [ ] 2.8 Identity: node ids from level path and name path, duplicate-name errors, positional ids and warnings for unnamed entities; `LevelInstance` component with owner chains
- [ ] 2.9 Confirm tsx T3: `override` provenance with location; refuse an override of a bound field with a load error naming file, line, site and expression; test it, including a bound root `Transform`

## 3. Writer

- [ ] 3.1 Operations on name paths (`set`, `revert`, `set_prop`, `add_instance`, `add_entity`, `delete`, `rename`, `reparent`) as text splices with style copied from siblings, and inverse batches
- [ ] 3.2 Value printing: shortest round-trip floats, short type paths unless the file writes the type in full, only leaf fields that differ
- [ ] 3.3 Re-parse check after every splice and atomic writes; golden diff tests asserting a one-field edit is one changed line
- [ ] 3.4 Naming unnamed entities on first write; `rename` updates the retained tree before the reload
- [ ] 3.5 Base-hash check, reload of the disk version, name-path replay, `rebased` and `conflict` results; tests for the agent-edits-mid-drag and target-deleted scenarios
- [ ] 3.6 Sidecar levels for `app.scene` roots on a `computed` rejection (needs tsx T4)

## 4. Hot reload

- [ ] 4.1 Watch level files through Bevy's file watcher; reconcile a file and its includers at one frame boundary; recognise the writer's own writes by hash
- [ ] 4.2 Tests for every level hot-reload rule in design.md: unchanged overrides kept, removed overrides reverted, prop change keeps `initial*`, reorder respawns nothing, tombstones, bad file changes nothing
- [ ] 4.3 Re-expand all instances inside each TS generation and fail the generation when a level fails under it

## 5. Viewport routes and protocol

- [ ] 5.1 Confirm viewport V1; compute the owner chain and target per field under `levels.prefabEdits` (`override` default, `source`), and turn `bsn` edits into writer batches
- [ ] 5.2 `level_apply` to the next owner inward: a two-file transaction between `.bsn` files, and the tsx-route transaction that removes the override only on `written`
- [ ] 5.3 Add `bevy-ts/level_list`, `level_node`, `level_write`, `level_apply` and `level+watch` to `protocol/bevy-ts.openrpc.json` as 1.2; regenerate types; conformance tests

## 6. First demo

- [ ] 6.1 Move `examples/arena/` enemies into `assets/levels/arena.bsn` with `Enemy` as a registered prefab and `room.bsn` included twice
- [ ] 6.2 Walk the seven demo steps in design.md under both `prefabEdits` policies, recording `git diff` output for each write
- [ ] 6.3 Measure load, reload and expansion of a level with 1,000 and 10,000 instances, and writer latency per edit; record them in `docs/`
