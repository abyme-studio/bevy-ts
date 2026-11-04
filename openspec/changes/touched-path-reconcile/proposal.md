## Why

An edit on a 20,000-entity level takes 358 to 447 ms (`docs/research/scene-expansion-cache.md`, branch `scene-expansion-cache`, three runs). The writer is 36 to 48 ms of it. The rest is the reconcile after the write: compose the whole level (165 to 276 ms), expand every instance (52 to 65), resolve and diff every node (83 to 86). The edit changed one entity. The writer knows which: it names every path it touched. A drag release should answer in under 50 ms at 20,000 entities.

## What Changes

- **A write reconciles only what it touched.** After a write, levels composes, expands, resolves and reconciles only the subtrees at the paths the batch touched, in each open level the file is in. Everything else keeps its entities and change ticks and is not read.
- **The writer names its units.** Each op gives the name paths it changed: `set`, `revert`, the prop ops and the component ops their entity; `add` and `restore` the new entity; `delete` the removed one; `reparent` the old and the new place; `rename` and the positional names after it, the new names.
- **The unit rises to the instance.** A path inside a scene component instance (an override of a prefab's inner entity) reconciles the whole instance, since its children come from one expansion.
- **Partial compose.** levels composes a level filtered to key paths: it walks the file and its includes down to each unit and builds only those subtrees, with their ancestors' frames. A missing unit composes to nothing, which despawns it.
- **The full path stays**, and a write takes it when a partial one can't be trusted: the batch adds or removes an include, the file's single top changes, the root is a sidecar, the root is dirty, failed last time or has no kept tree, a unit's old or new subtree holds callbacks (binding, handler, ref, slot), a placement can't map a unit, or the partial compose fails. The full path is today's.
- **The asset takes the writer's parse.** The writer already parses the new text to verify it; the level asset is built from that parse, not a second one.
- **tsx:** `apply_at` with `reconcile` places a new node at its described index, also in the kept tree; `rekey` moves the kept tree's node with its entity.
- **Edit steps are recorded.** levels keeps the last write's time per step (writer, parse, compose, expand, reconcile, stamp) and how many instances it expanded and nodes it checked. The bench and the tests read it; the protocol does not.
- **A bench** splits an edit's time by step, the full path against the partial, at 2,000 and 20,000 entities.
- **Out of scope:** external changes (a file saved by an agent or editor) still reconcile the whole level; sidecar levels; the writer's own 36 ms; the expansion cache (parked).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `level-writing`: a write reconciles only the paths it touched; when it takes the full path.
- `levels`: composing a level at key paths; a level's per-path state (owners, scenes, placeholders, warnings, assets) patched per unit.
- `scene-reconciler`: `apply_at`'s placement of new nodes in the kept tree; `rekey` moves the kept tree.

## Impact

- `crates/bevy-ts-levels`: `writer.rs` (`Written.units`, `Written.parsed`), `compose.rs` (`prepare_at`, per-path warnings), `edit.rs` (`write_inner` reconciles units, `EditSteps`), `engine.rs` (`reconcile_at`, per-unit patching of `LevelRoot`, warnings and assets by key path).
- `crates/bevy-ts-tsx`: `reconcile.rs` (`apply_at` placement in the kept tree, a subtree callback check), `lib.rs` (`rekey` moves the kept node; a `reconcile_external_at` entry).
- No protocol change: `level+watch` and `tsx_tree+watch` send the same messages, for fewer nodes.
- Tests: `crates/bevy-ts-levels` unit tests for units and `prepare_at`; `crates/bevy-ts-levels/tests/it/levels.rs` for the partial reconcile and its equivalence with the full path; `crates/bevy-ts-tsx/tests/it/reconcile.rs` for placement and rekey.
- Bench: `bench/edit-split/` (new crate, outside the workspace, as `docs/testing.md` rule 8 says). Results in `docs/research/touched-path-reconcile.md`.
- Docs: `docs/levels.md` (what a write reconciles, when it takes the full path).
- Builds on compose's `prepare`/`finish` split and its per-entry build as main has them. compose-regression did not land, so the full path keeps today's 165 to 276 ms of compose at 20,000 entities.
- Needs the operator's go:
  - **Performance: an edit's cost changes.** Target under 50 ms at 20,000 entities and under 8 ms at 2,000 (today 358 to 447 ms and about 26 ms). The full path costs under 1 ms more than today (deciding the units). The trade-off: a partial reconcile that misses a dependency leaves the world stale until the next full reconcile (a save from outside, a reload). The equivalence test runs every op kind against the full path to guard it.
  - **A new pattern: partial compose.** levels composes a level filtered to key paths, a second way into compose next to the whole-level one. The alternative is the expansion cache (parked), which takes about 50 ms off and leaves compose and the diff whole.
  - **Edit step timings in shipped code:** an `EditSteps` resource, a few `Instant` reads per write. The alternative is a bench that calls the steps itself, which drifts from the real path.
- No new dependency.
