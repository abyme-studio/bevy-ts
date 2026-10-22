## Context

play-stop (play-snapshot, landing first) makes Stop a `restart` fed the live set. `crates/bevy-ts-viewport/src/edit.rs` `stop()`: `StateTransition` (native `OnExit(Play)`), `enter_edit`, `generation::rebuild`, `levels::restore_open`, `levels::settle`, `runtime::reset_clock`, `follow_selection`, then the answer.

In the rebuild, tsx's restart handler (`bevy-ts-tsx/src/lib.rs:396`) despawns every `TsxNode` and resets `TsxTree`. levels' `after_restart` and `after_generation` (`bevy-ts-levels/src/engine.rs`) mark every root dirty, so `settle` composes, expands, resolves and spawns every level again. `docs/research/stop-incremental.md` measured that at 10,000 instances (20,000 entities): 649 ms, of which the stamp is 252 (quadratic, fixed separately), expansion 171, and despawn, resolve and spawn about 105.

What already holds:
- Node ids are stable: `node_id(root, key_path)` (`desc.rs:96`). A level's key path is its name path.
- `LevelRoot.applied` keeps each file's hash as of the last apply.
- `resolve_tree` produces `RNode`s with `Arc<[Resolved]>` components; `apply_root` applies a whole root.
- `generation::start_game` is the one place the game starts: Play and a step before start both call it.
- TS writes mark Bevy's per-row change tick, so one scan sees TS and Rust writes (research arm C: 0.32 ms at 10,000).

## Goals / Non-Goals

**Goals:**
- Stop under 100 ms at 20,000 level entities when the play changed few of them; about 115 ms when it changed all.
- After a Stop the world is the one the source describes, as play-stop promises.
- One owner for partial applies: tsx's `apply_at`.

**Non-Goals:**
- The `stamp_instances` fix (its own commit, before this).
- The content-keyed expansion cache: the next stage. It makes the full path cheap and lets an instance with callbacks re-expand alone.
- Moving levels' write path onto `apply_at` (levels-scale).
- Writing values back in place instead of respawning a changed subtree. Measure first.
- Keeping generation scenes (`app.scene` roots). They remount as today; they are small and hold callbacks.

## Decisions

1. **The baseline is taken when the game starts, not at Play.** A step before the first Play runs `Startup` and frames in edit; those changes must be undone too. `start_game` is the one entry. bevy-ts gains `on_ts_start` (plugin-api), called before `run_startups`; tsx registers it and records, per external root then applied, the world change tick, `entity → ArchetypeId` for each node, and the root's apply count. Alternative: record in `enter_play`. Rejected: misses steps in edit. Alternative: record ticks at every apply. Rejected: Bevy recomputes `GlobalTransform` and similar the frame after a spawn, so every fresh entity would look changed; at game start those have settled.

2. **The scan.** For each node of a kept root: changed if its archetype differs from the recorded one, or any component's added or changed tick is newer than the baseline tick. Removal shows as an archetype change. A changed node marks its top-most changed ancestor's subtree for respawn; the unit restored is the subtree under the highest changed node. Runs in `stop()` before `rebuild`, so the rebuild's despawn of `TsSpawned` children does not mark parents. A derived component Bevy writes every frame would mark every entity: correct (restoring too much is only slower), and task 6.3 checks it with rendering on.

3. **Which roots are kept is levels' decision; how is tsx's.** `levels::plan_stop(world) -> StopPlan` runs first in `stop()`:
   - kept: an open, non-sidecar level root with a tsx baseline, the same apply count, not dirty, each file in `applied` hashing the same on disk (one read and FNV per file, about 1 ms for 600 KB), and no callbacks (tsx answers: no binding, handler, ref or slot under the root).
   - everything else: full path.
   The plan holds, per kept root, the changed key paths from tsx's scan and its tombstoned key paths. It is a resource tsx owns (`bevy_ts_tsx::StopPlan`), inserted for the rebuild and removed after the restore.

4. **The rebuild keeps the plan's roots.** tsx's restart handler despawns every `TsxNode` except those of kept roots, and keeps those roots' `RootState`, entity map entries and kept resolved trees; it drops their tombstones into the plan (they will respawn) and drops every baseline. levels' `after_restart` and `after_generation` leave kept roots clean (not dirty, `applied` kept). The rebuild's generation apply gets the same `ComponentId`s for an unchanged layout (`Planned::Reuse`), so the kept entities' TS columns stay valid. A kept root whose component layout changed cannot happen: the generation set is the same one (a reload during play makes the root dirty, so not kept).

5. **The restore.** After `restore_open`, for each kept root, `apply_at(root, changed ∪ tombstoned paths, Respawn)` from the kept resolved tree, then levels stamps the respawned nodes only. `settle` then runs the full path for the rest, as play-stop does. Order of `stop()`:
   1. `plan_stop` (scan, file hashes)
   2. `StateTransition`, `enter_edit`
   3. `generation::rebuild` (honours the plan)
   4. `restore_open`
   5. restore kept roots (`apply_at`, stamp)
   6. `settle`
   7. `reset_clock`, `follow_selection`, the answer

   Scanning before `OnExit(Play)` means a native crate's reset in `OnExit` counts as a change if it writes a level entity. That is correct: the entity is respawned from the description.

6. **`apply_at` is one primitive with two modes.** `apply_at(world, root, items: &[(Vec<String>, Option<&RNode>)], mode)`. `Respawn` despawns the live subtree (if any), spawns the given one under its described parent at its described index, clears tombstones, updates `TsxTree` and the kept tree. `Reconcile` runs today's per-node diff on the subtree. `None` despawns. `apply_root` stays the whole-root path; both share the per-node code. levels-scale's write path will call `Reconcile`; this change only uses `Respawn`. Alternative: a Stop-only respawn helper. Rejected: two partial-apply paths would drift (one owner).

7. **The kept resolved tree.** `RootState` gains `resolved: Option<Arc<[RNode]>>` for external roots, set by `apply_root`, patched by `apply_at`. Components are already `Arc<[Resolved]>` shared with the template cache, so 20,000 nodes cost the node structs and paths, a few MB. Generation scenes don't keep one.

8. **Callbacks force the full path, per level.** A binding, handler, ref or slot holds an id into the old isolate. Re-expanding only those instances needs per-instance compose state, which the expansion cache stage brings. Until then the whole level remounts. The bench's `Grunt` has none; a level with clickable enemies does, and gets play-stop's cost.

## Risks / Trade-offs

- [A Rust system relying on `Added` after a Stop sees nothing for kept entities] → Bevy's meaning: nothing changed. `docs/native.md` says reset in `OnExit(Play)` and not to rely on re-adds. Rapier's entities are changed in play (bodies move, handles insert), so they are respawned.
- [Hooks and observers don't fire for kept entities] → state kept by add/remove pairs stays paired either way. TS observers are cleared at `enter_play`.
- [A derived component marks every entity each frame under rendering] → everything is respawned, about 115 ms; task 6.3 measures it. If it happens, a follow-up decides from Bevy's source, not a skip list guessed here.
- [A file changed on disk but the watcher had not fired] → the hash check catches it.
- [The scan misses a change] → only a write that bypasses change detection (`bypass_change_detection`, raw pointers). bevy-ts's TS write path marks ticks (`systems.rs`); a test writes from TS and from Rust.
- [Memory for kept trees] → a few MB at 20,000 entities; measured in task 6.

## Migration Plan

Lands after play-stop and the stamp fix. No protocol or file format change. Rollback: revert; Stop is play-stop's full remount again.

## Open Questions

- Does Bevy 0.20 mark `ViewVisibility` or `Aabb` changed every frame under rendering? Task 6.3.
