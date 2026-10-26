## Why

After stop-incremental's restore, a Stop at 20,000 level entities takes 98 ms with nothing touched, 131 ms with 100 enemies touched and 207 ms with all of them touched (`docs/research/stop-cost.md`, third round, medians of 3). The rebuild is 87 to 113 ms of each, and the respawn adds about 140 ms when the play touched every entity. The goal is under 100 ms in every arm.

The rebuild grows with the level (37 ms at 2,000 entities, 108 at 20,000) though it no longer despawns the level. Three parts of it are known. levels' section check prepares every level again (about 30 ms at 20,000, `docs/research/stop-incremental.md`). The JS engine boots (about 9 ms). The generation loads (about 9 ms). The rest is not split yet.

## What Changes

- **The rebuild becomes named steps** that the bench times one by one: reset (despawns, restart handlers), engine (drop the old JS engine, boot a new one), load (the module set evaluated, declarations checked), sections (plugin section checks), apply (sections applied, systems), and `Startup`. Each Stop arm reports them, 3 runs at 2,000 and 20,000 entities. The first task finds where the unsplit time goes before anything else is cut.
- **A Stop skips the level check for the levels it keeps.** The rebuild re-applies the live module set. That set already passed the check with these files, and the Stop's plan has just hashed them unchanged. Levels the Stop does not keep are still checked.
- **A spare JS engine is booted ahead.** After a restart or a Stop, bevy-ts boots the next engine in a later frame. A restart takes the spare instead of booting one, and the old engine is dropped in the next frame, after the Stop answers. The spare runs no code until it is used, so a restart still gets a fresh instance.
- **A changed entity is written back instead of respawned when it can be.** If its archetype is unchanged and every component the play changed is one tsx inserted, or one Bevy recomputes from them (`GlobalTransform` and the derived list), the Stop inserts those components again with the description's values. The entity keeps its id. Anything else still respawns. A play that only moves entities, the bench's all-touched arm, then restores without despawning or spawning.
- **Expected at 20,000 entities:** about 40 ms with nothing touched, 45 with 100 touched, 60 with all touched. A play that adds or removes components on every entity still respawns them all, about 170 ms. That is out of this change.
- **Next stage, unchanged:** the content-keyed expansion cache, for the full path.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `edit-mode`: Stop writes a changed entity back in place when it can, keeping its id.
- `scene-reconciler`: the changed-node answer says which components changed; writing a node back from the kept tree.
- `level-hot-reload`: a Stop's rebuild does not check the levels it keeps.
- `hot-reload`: a restart takes an engine booted ahead; the old one is dropped after.

## Impact

- `crates/bevy-ts`: `generation::apply_set`'s restart branch split into step functions; `ScriptBackend` gains `prepare_restart` and the restart takes a prepared engine; a `Last` system boots the spare and drops the old engine.
- `crates/bevy-ts-v8`: `V8Backend` holds an optional spare `JsRuntime` and a retired one to drop; the inspector moves to the new runtime as today.
- `crates/bevy-ts-tsx`: `restore::changed` answers the changed components per node; `write_back(world, root, nodes)`; the recomputed list next to `restore::derived`.
- `crates/bevy-ts-levels`: `check_section` skips the levels in the `StopPlan`; `restore` writes back or respawns per node and stamps only respawned ones.
- `crates/bevy-ts-cli/tests/it/stop_bench.rs`: times the rebuild's steps; `docs/research/stop-cost.md` gets the table.
- No protocol change. No new dependency.
- Needs the operator's go, though lead@bevy-ts cleared the stage under the bevy delegation:
  - **A new mechanism: the spare engine.** One booted V8 runtime held in memory at all times under `bevy-ts dev`, a few MB. The boot takes about 9 ms in a later edit frame instead of in the Stop. The alternative is to boot it in the Stop as today.
  - **Write-back.** A written-back entity keeps its id. Bevy's replace and insert hooks and observers fire for the components written. Today's respawn fires remove, despawn, add and insert hooks. The alternative is the respawn, about 140 ms at 20,000.
  - **Performance:** the Stop's numbers above, and a frame of about 9 ms in edit after each Stop or restart while the spare boots.
