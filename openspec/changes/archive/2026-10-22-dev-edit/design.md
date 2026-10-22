## Context

Today `bevy-ts dev` applies the first generation and runs its TS `Startup` inside `generation::apply` (`run_startups`). The studio then calls `viewport_mode { mode: "edit" }` once `hello` answers. By then `Startup` and some frames have run. Edit mode is `ScriptClock.paused` plus `Time<Virtual>` paused (`bevy-ts-viewport/src/edit.rs`, `enter_edit`). TS observers queue in `ObserverQueue` while paused and fire on resume. Levels from `app.level()` reconcile in the `tsx` section's apply and the levels plugin's `Update` `watch`, which run whatever the clock says. Native systems run regardless; `FixedUpdate` stops because `Time<Virtual>` is paused.

Bevy has no editor play mode (`bevy_editor_prototypes` is archived). Bevy's tool for "these systems run only in some mode" is a `States` with `in_state` run conditions and `OnEnter`/`OnExit`.

## Goals / Non-Goals

**Goals:**
- An engine that boots in edit mode with no TS game code run, the scene from `app.level()` shown.
- One state other code can read and hook: native crates, and E4 later.
- No protocol method added; the studio's Play is today's `viewport_mode play`.

**Non-Goals:**
- `level_open`/`level_close` (E2). An opened level reconciles like any root; this change's rule (no game code in edit) applies to it unchanged.
- Snapshot on Play and restore on Stop (E4).
- Gating native systems bevy-ts did not add.

## Decisions

1. **The mode is a Bevy state, `bevy_ts::EngineMode`.** It lives in `bevy-ts`, not `bevy-ts-viewport`, so a native crate and a shipped game see it. `bevy-ts-viewport`'s `Mode` stays the source of the switch; `set_mode` sets `NextState<EngineMode>`. Native code gates with `run_if(in_state(EngineMode::Play))`, the Bevy way. Alternative: a bevy-ts run condition `playing()`. Rejected: a state also gives `OnEnter`/`OnExit`, which E4 needs.

2. **`--edit` enters edit before the first frame.** `run_dev` inserts `EngineMode::Edit` as the initial state and sets `ScriptClock.paused` before `app.run()`. The viewport crate's `Startup` runs `enter_edit` once when the initial state is `Edit`, so the editor camera, gizmos and picking come up exactly as a later switch makes them. The editor camera's first pose is taken from the first active game camera on the primary window when one exists by then; otherwise the default pose (`(-6, 5, 10)` looking at the origin). A level's camera spawned later does not move it.

3. **`Startup` waits for `Generations.started`.** `Generations` gains `started: bool`, true from boot without `--edit`. `run_startups` runs nothing while it is false. The first `enter_play`, or the first `step`, sets it and runs the due `Startup` systems in the same frame, before the stages. A `restart` reload in edit sets it false. `ViewportModeResult.started` reads it.

4. **Triggers are cleared, not queued, in edit.** Each frame in edit mode `ObserverQueue` is emptied in `First`. Alternative: keep them and fire on Play so `Add` observers see the level's entities. Rejected: edits in edit mode would fire game code later, and Unity runs no game code for edit-time changes. Game code reads the level's entities in `Startup`, which runs after the level loaded.

5. **`pause false` before start fails.** Resuming stages that never had `Startup` runs `Update` on an unstarted game. Play is the only way to start; `step` starts it too, as Unity's Step from edit does.

6. **Rust rebuilds come back in edit.** The supervisor passes the raw args, `--edit` included, to the new engine. That matches Unity's "Stop Playing and Recompile". No extra state crosses processes.

## Seam for E4

E4 hooks `OnEnter(EngineMode::Play)` to snapshot the world (or reload the open levels) and `OnExit(EngineMode::Play)` to restore and set `started` false. Nothing here needs to change for that: `started` already lets `Startup` run again on the next Play.

## Risks / Trade-offs

- [Native systems the crate author does not gate keep running in edit] → documented in `docs/native.md`; rapier steps in `FixedUpdate`, already paused.
- [Prefab `effect`s are user code that runs in edit] → they build the scene; kept, as `scene()` must run to show it.
- [Dropping edit-time triggers changes today's edit mode] → in the proposal's Impact for the operator's go.
- [An older engine rejects `--edit` with "unknown option"] → the studio gates the flag on its pinned `BEVY_TS`, agreed with build@bevy-game-shell.

## Protocol

Agreed with build@bevy-game-shell (lead@abyme named it owner of the studio's play mode): `ViewportModeResult.started`, additions only, the next free minor on main at landing (1.18.0 if first, else 1.19.0 after rebasing on level-open). The game shell's Play/Stop, Pause and Step keep calling `viewport_mode`, `pause` and `step`.
