## Context

E1 (`dev --edit`) and E2 (`level_open`) are on main. Play and Stop today, in `crates/bevy-ts-viewport/src/edit.rs`:

- `set_mode` (281) sets `NextState<EngineMode>`, runs `StateTransition` out of band, then `enter_play` (406) or `enter_edit` (340).
- `enter_play` restores the game cameras, turns the editor camera and gizmos off, clears the `ObserverQueue`, calls `start_game` (`generation.rs:123`, `Startup` once) and unpauses.
- `enter_edit` pauses and swaps the cameras. The world stays as play left it.

The `restart` reload already does most of a Stop (`generation.rs:148-170`): it despawns `TsSpawned`, clears `startups_run`, sets `started = false` in edit, resets `SystemMemory` and `TsPayloads` (messages and states, `messages.rs:150`), runs the restart handlers, clears `TsSystems` and gives V8 a fresh isolate. tsx's handler (`bevy-ts-tsx/src/lib.rs:396`) despawns every `TsxNode` and clears the tree. levels' (`bevy-ts-levels/src/engine.rs:372`) marks every root dirty and clears `applied`. Three gaps:

- `reload_now` (`generation.rs:493`) recompiles. The applied `ModuleSet` is not kept anywhere once `backend.load` has it.
- TS resources survive: `insert_resources` (`generation.rs:402`) skips one that exists.
- Levels re-expand in `watch`, next frame, not in the call.

Research (play-snapshot/research) compared a snapshot with a rebuild. The engine holds nothing unsaved. A snapshot can't hold V8's heap, and Bevy's cloner skips TS components (`ComponentCloneBehavior::Default` on a dynamic layout, `components.rs:156-163`). A reconcile keeps values systems changed, so it can't restore. Unity re-reads the scene on Stop. The rebuild was settled by lead@play-snapshot.

## Goals / Non-Goals

**Goals:**
- Stop puts the world back as the source describes it, every time, from the newest live code.
- One reset path shared by `restart` and Stop.
- The studio learns every mode change from a stream, and a Play that can't run says why.
- Stop's cost is measured at 1k and 10k instances.

**Non-Goals:**
- An in-memory snapshot or world clone.
- Resetting Rust state bevy-ts did not create.
- The game camera while editing (stage 2 below).
- Keeping play-time changes on Stop (Unity's "copy component values"). Nothing asks for it.

## Decisions

1. **The rebuild is `generation::rebuild(world)`, the restart branch of `apply` fed the live set.** `apply` stores a clone of the set it applied in a `LiveSet` resource (`ModuleSet` is `Clone`; a set is the compiled JS text, a few hundred KB at most). `rebuild` calls `apply(world, live.clone(), true)`. One owner: `apply`'s restart branch. Alternative: recompile on Stop. Rejected: a compile takes seconds on a large project and might fail on a file mid-edit; Stop must not depend on the editor's buffers.

2. **`restart` despawns TS resource entities.** In the restart branch, entities with `IsResource` whose component is a TS component are despawned before the handlers run, so `insert_resources` inserts them again from the manifest. A resource a generation declares without `insertResource` is then absent, as at first load. Rust resources are untouched. Alternative: only on Stop. Rejected: a restart is "as if first", and two reset paths would drift.

3. **Stop's order, in `set_mode`, for `play` → `edit`:**
   1. Record the open levels and their openers (from step 0 of the Play, see 4).
   2. `NextState(Edit)` and `StateTransition`: native `OnExit(Play)` runs first, so a crate tears down before bevy-ts rebuilds.
   3. `enter_edit`: pause, cameras swap. The editor camera is `EditorOnly`, not `TsSpawned`, so the rebuild leaves it.
   4. `generation::rebuild`. Because the state is already `Edit`, `started` becomes false.
   5. `levels::restore_open(set)` then `levels::settle(world)`: reconcile every dirty level now, as `open_now` does.
   6. Reset `Time<Virtual>` (a fresh one, keeping `max_delta` and relative speed, paused), `Time<Fixed>`'s overstep, and `ScriptFrame`.
   7. The selection re-resolves (5), one `viewport_mode+watch` message, the answer.

   Alternative: run the rebuild in `OnExit(Play)` as a system. Rejected: the reply must come after the rebuild, and an exclusive call in `set_mode` gives that with no deferred reply.

4. **The open-level set is a value levels owns.** `bevy_ts_levels::open_set(world) -> Vec<(String, Opener)>` at Play; `restore_open(world, &set)` at Stop closes the levels not in it and opens the missing ones with their opener (`open_as`, sidecars with their scene). It does not touch the editor's `closed` set except to reopen. viewport keeps the set in `EditState` between Play and Stop. A `ts` level the live generation no longer declares is not reopened; the generation's own section decides.

5. **The selection follows by node id, after the settle.** `keep_selection` already follows a node id to its new entity. Today it runs as a system and could run in a frame where the old entity is gone and the new one not spawned. Stop calls the same follow-up directly after step 5, so the remount is there. Level node ids come from the level path and name path, so they are stable across the rebuild. A TS-spawned entity has no node id and is dropped.

6. **A Play fails before anything changes.** `set_mode` checks first: `Generations.live == 0` and no compile has finished → `no-generation`; the newest `ReloadLog` entry is `failed` → `compile-errors`, with the first error's `file:line: message` as the JSON-RPC message. A compile still running doesn't block (`compiling` is not `failed`). Unity refuses Play with compiler errors; Godot runs the old build. The studio asked to refuse (build@studio-edit-and-levels, 2026-10-22).

7. **The watch is viewport's.** `ModeWatch { seq, last: ModeJson, cause }` in viewport, compared each frame in `Last` against `mode_json`, as `viewport_tools+watch` follows its tools. A call sets the cause before the change lands; a change with no cause set is `restart` when `Generations` shows a restart since the last frame, else `client`. Stop sends one message because it changes everything in one call. `pause` and `step` live in `bevy-ts-remote`; they don't need to know the watch, the frame compare catches them.

8. **Native crates reset themselves.** The Bevy way is a state's `OnExit`. bevy-ts can't know what a crate's resource means. `docs/native.md` gets an example: reset a resource and despawn spawned entities in `OnExit(EngineMode::Play)`. Rapier: bodies on TS entities go with their entities; the port gets a check that play, Stop, play runs the same.

## Risks / Trade-offs

- [Every Stop is a restart: fresh isolate, generation applied, levels remounted] → Task 5 measures Stop at 1k and 10k level instances in `docs/research/stop-cost.md`. Expected from `levels-scale.md`: 37 ms and 271 ms for levels alone, plus apply. If 10k is over 1 s, the follow-up is the incremental reconcile `levels-scale.md` already names, not a snapshot.
- [Behaviour change: edit then play no longer resumes] → `pause` is the freeze. The studio's B4 calls Stop only from its Stop button. The studio's one-time switch to edit rebuilds a game played outside abyme; abyme opens a project in edit, so that is intended.
- [A re-apply of the live set fails, e.g. a backend error] → the set applied once, so this is unexpected. `apply` already never leaves a generation half applied; on failure Stop answers in edit with the empty world and the error on the log and `reload+watch`, as a failed first load. A test forces it with the fake backend.
- [Transient gizmo edits are lost on Stop] → already the rule for `restart` (viewport-edits). They were never on disk.
- [A native crate that doesn't reset leaks state into the next Play] → documented; bevy-ts can't own it.
- [`Time<Virtual>` replaced while a Rust system holds expectations of monotonic time] → only in edit, paused; native `OnExit(Play)` runs first.

## Migration Plan

Additions to the protocol only, at the next free minor on main when this lands (1.20.0 if first; lead@bevy-ts names it). The studio checks the minor from `hello` before subscribing to the watch. An older studio sees the new Stop behaviour with no schema change. Rollback is reverting the change.

## Stage 2, later: the game camera while editing

E4 as approved also picked the camera apart from the mode, so a Game tab could show the game's camera while editing. Left out of this change:

- The studio's Game tab is one view today (build@studio-edit-and-levels), so B4 doesn't need it.
- Under `--edit` the game camera usually comes from `Startup`, so it doesn't exist before Play.
- Bevy's way is a second camera rendering to `RenderTarget::Image`, copied into a second shared surface for abyme, alongside the editor camera's. That is new infrastructure (a second surface ring per game viewport) and costs a second render of the scene every frame.

It needs the operator's go, then its own change: `viewport_attach { view: "game-camera" }` or similar, and the cost measured.

## Open Questions

None the code leaves open. Answers to research's questions:

- a) The selection re-resolves by node id, which for level entities is the level path and name path (decision 5).
- b) The live set is not kept today; `apply` keeps a clone (decision 1).
- c) Play fails on `no-generation` and `compile-errors` (decision 6). A `Startup` throw stays a logged error.
- d) Yes, `Time<Virtual>`, `Time<Fixed>` and the frame count reset on Stop.
- e) No. A `restart` reload keeps the mode; in play it reruns `Startup`, as today.
