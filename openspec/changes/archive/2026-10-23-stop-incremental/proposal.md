## Why

A Stop takes 649 ms at 20,000 level entities (`docs/research/stop-incremental.md`, 10,000 instances). play-stop's Stop is a `restart`: tsx despawns every level entity and every level composes, expands, resolves and spawns again, though the level description is the one already applied. Only what the play changed differs from it, and Bevy's change ticks name that directly. The studio's Stop button should answer in under 100 ms.

## What Changes

- **Stop restores, it does not respawn.** A level that kept its code and its files during the play keeps its entities. Stop respawns only the subtrees the play changed and the nodes it despawned, from the level's last resolved tree. The world after a Stop is still the one the source describes.
- **The game-start baseline.** When the game starts (a Play, or a step before the game started) tsx records, for each level root, the change tick and each entity's archetype. At Stop, a level entity is changed when one of its components changed after that tick, or its archetype differs.
- **Every tombstone of a kept level is restored** at Stop, as play-stop's rebuild does today.
- **A level takes the full path when the restore can't be trusted:** its file or an included file differs from what it was composed from; it was applied again after the game started (a level or code reload during play); it is a scene's sidecar; its description holds JS callbacks (bindings, handlers, refs, `<Show>`/`<For>`), which die with the isolate; or there is no baseline (a game booted in play). The full path is play-stop's: remount as if first.
- **A tsx primitive: apply a root at key paths.** `apply_at(root, paths, mode)` reconciles or respawns only the subtrees at the given key paths, keeping the rest of the root. tsx owns it. Stop uses it to respawn; levels-scale's write path will use it to reconcile only the paths a write touched.
- **tsx keeps each level root's resolved tree** after applying it, so a Stop restores without composing or expanding.
- **Plugin API: a start notification**, the same shape as the restart notification, so tsx takes the baseline before `Startup` runs.
- **Behaviour a kept entity changes:** its entity id is kept, no hooks or observers fire, and a Rust system sees no new `Added` for it. Bevy's meaning: nothing changed. Documented in `docs/native.md`.
- **Spec line that changes:** tsx-hot-reload's "restart clears every tombstone and the retained tree, so every scene mounts as if first" stays for `restart`; a Stop restores the tree as the source describes it, keeping what the play did not change.
- **Out of scope, their own stages:**
  - the quadratic `stamp_instances` fix (build@stop-incremental, off main, before this);
  - **next stage:** the content-keyed expansion cache, which makes the full path cheap and lets an instance with JS callbacks re-expand alone instead of its whole level;
  - levels' write path moving onto `apply_at` (levels-scale).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `edit-mode`: Stop restores the level entities the play changed instead of remounting every level; when a level takes the full path.
- `tsx-hot-reload`: a Stop restores tombstones and the tree as the source describes them; `restart` unchanged.
- `level-hot-reload`: the same for a level's tombstones.
- `scene-reconciler`: apply a root at key paths; the resolved tree kept for external roots; the game-start baseline and the touched scan.
- `plugin-api`: the start notification.

## Impact

- `crates/bevy-ts`: `plugin_api.rs` `on_ts_start`; `generation::start_game` notifies before `run_startups`.
- `crates/bevy-ts-tsx`: the baseline (`on_ts_start`), the touched scan, a kept `Arc` resolved tree per external root, `apply_at`, and the restart handler keeping the roots a Stop plan names.
- `crates/bevy-ts-levels`: the Stop plan (which roots are kept: file hashes against `applied`, no apply since the baseline, no callbacks, not a sidecar); `after_generation` and `after_restart` leave kept roots clean; stamping only the restored nodes.
- `crates/bevy-ts-viewport`: `edit.rs` `stop()` plans before `generation::rebuild` and restores after `restore_open`.
- No protocol change: `tsx_tree+watch` and `level+watch` report what was respawned, with the same messages.
- Tests: tsx unit tests for `apply_at` and the scan; `crates/bevy-ts-viewport/tests/it/edit.rs` Stop scenarios; `stop_bench.rs` at 2k and 20k entities.
- Docs: `docs/viewport.md` (what Stop keeps), `docs/native.md` (no `Added` for kept entities), `docs/research/stop-cost.md` (the measurements).
- Builds on play-stop (play-snapshot), which lands first.
- Needs the operator's go:
  - **A new mechanism: the game-start baseline**, and a resolved tree kept in memory per level root (a few MB at 20,000 entities). The alternative is play-stop's full remount (649 ms) or the expansion cache alone (about 190 ms).
  - **Stop's performance and behaviour change:** about 50 ms at 20,000 entities when the play changed few of them, about 115 ms when it changed all. Kept entities keep their ids and fire no `Added`, hooks or observers.
  - **A new plugin API hook**, `on_ts_start`. It follows `on_ts_restart`'s pattern. The alternative is viewport and remote's `step` each calling tsx before `start_game`, two callers that can drift.
- No new dependency.
