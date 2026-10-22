## Context

play-stop (1.24) made Stop rebuild the edit world from source and added `viewport_mode+watch`. It left out "the game camera while editing" because its approved form was a second camera on `RenderTarget::Image` and a second surface to abyme: new infrastructure. The studio (lead@studio-play-stop) agreed a smaller form: swap the one view between the editor camera and the game cameras. Nothing renders twice.

Today, in `crates/bevy-ts-viewport/src/edit.rs`:
- `EditState` holds the mode, `ui`, `gamepadUnfocused`, the game cameras with their active flags at edit entry (`game_cameras`), and the editor camera.
- `enter_edit` turns every game camera off and the editor camera and overlay cameras on; `enter_play` restores the flags and turns every `EditorOnly` camera off. `camera_added` turns off a game camera spawned in edit and records its flag. All go through `Pending`, applied in `First`, because a camera switched in `RemoteLast` panics or blanks a frame.
- The tool systems (`edit.rs` Update chain, `edits.rs` PostUpdate, `orient.rs`) and the `EditorGizmos` config run on `mode == Edit`.
- `apply_ui` gives `IsDefaultUiCamera` to `UiSink` (edit, no `ui`) or the editor camera (edit with `ui`).

## Goals / Non-Goals

**Goals:**
- One choice, `camera`, separate from the mode, kept by the engine across Play and Stop.
- The game view shows exactly what Play would draw at that frame, paused.
- A 1.24 client sees no change.

**Non-Goals:**
- Two panes, editor and game side by side.
- Picking, gizmos or edits in the game view.
- An editor camera in Play (refused for now; Unity's Scene view during Play is a later stage with the two panes).

## Decisions

**The choice is a field on `EditState`, the view is derived.** `EditState.camera_choice: CameraChoice { Auto, Editor, Game }`, default `Auto`. The view in force is `Game` when the mode is Play or the choice is `Game`, else `Editor`. One function, `EditState::editor_view()`, answers it, and every tool gate uses it instead of `mode == Edit`. The existing field `camera` (the editor camera entity) is renamed `editor_camera` so `camera` is not two things. Alternative: a third mode `edit-game`. Rejected: the mode drives Play, Stop, pause and `EngineMode`; the view does not, and mixing them makes Stop's "bring back the choice" a special case.

**`auto` stays a value.** It answers as `auto`, not as what it resolves to. The studio derives the view from `mode` and `camera`. This keeps the answer a pure echo of the choice and leaves room for `auto` to mean something richer later (the two panes). `editor` and `auto` act the same today.

**One swap function.** `apply_view(world)` sets each camera's wanted flag through `set_active` from the view: editor view → editor camera and overlay cameras on, game cameras off; game view → editor and overlay cameras off, each game camera to its remembered flag. `enter_edit` still records `game_cameras` and spawns the editor camera (inactive in the game view, with its first pose from the first active game camera, as today), then calls `apply_view`. `enter_play` sets its cameras as today. A `camera`-only call in edit calls `apply_view` and nothing else: no Stop, no pause change. `camera_added` records the flag and, in the game view, leaves the camera as it came.

**Gizmos and UI follow the view.** `EditorGizmos` enabled only in the editor view. `TransformGizmoFocus` removed on entering the game view, as `enter_play` does. `apply_ui`: in the game view, `UiSink` and the editor camera drop `IsDefaultUiCamera`, so the game's own UI camera rule applies, as in Play.

**No game camera clears.** A window with no active camera may not be cleared, and the shared surface would then keep the editor's last frame. In the game view with `gameCameras == 0`, a clear-only camera is turned on: an `EditorOnly` `Camera { order: -11 }` with an empty `RenderLayers` and the default `ClearColorConfig`, so it draws nothing but the game's `ClearColor`. Off whenever a game camera is on. The implementer checks first whether Bevy 0.20 already clears a camera-less window target; if it does, the clear camera is dropped. Alternative: draw "No cameras rendering" in the engine. Rejected: the text is the studio's, from `gameCameras: 0`.

**`gameCameras` from the world, not the list.** Counted fresh in `mode_json`: game cameras (`Without<EditorOnly>`, `Without<PreviewOnly>`) whose remembered flag is on in edit (`game_cameras` entry, alive), or whose `is_active` is on in Play (through `is_active`, so a pending switch counts). Under `--edit` before Play the world has only level and scene cameras, so that is what it counts.

**The watch's `cameras` cause.** `watch_mode` in `Last` already compares `mode_json` with the last one sent. When the only field that differs is `gameCameras` and no restart ran, the cause is `cameras`; otherwise as today. A reload or level edit that adds a camera reaches the studio without a new hook.

**The refusal.** `camera: "editor"` with a resulting mode of Play is checked first in `set_mode`, before any change, and answers a JSON-RPC invalid-params error (-32602) with the message "The editor camera is not available in Play." It records `ModeCause::Failed`, as a refused Play does. Alternative: reuse -32000 (`PLAY_REFUSED`). Rejected: its `data.reason` belongs to Play's own refusals.

**`viewport_pick` in the game view** answers -32602 "Picking needs the editor camera." rather than casting from a camera that is not on screen.

**Protocol.** `ViewportCamera` enum (`auto`, `editor`, `game`); `camera` optional on `ViewportModeParams`; `camera` and `gameCameras` required on `ViewportModeWatch`, optional on `ViewportModeResult` (absent from older engines: treat as `auto` and unknown); `cameras` added to `ViewportModeCause`. Additions only: the next free minor at landing (1.25.0 today), told to lead@bevy-ts.

## Risks / Trade-offs

- [A game camera on a non-window target (a render-to-texture camera) turns on with the others] → That is what Play does; the game view matches Play.
- [The first switch to the game view in a frame where a camera's shadow cascades were not built] → The swap goes through `Pending` in `First`, the same path that fixed this for mode switches.
- [A game whose UI picks the highest-order camera picks differently once the overlay cameras are off] → Matches Play, which is the goal; a test draws the arena HUD in the game view.
- [`auto` and `editor` act the same, so a client may not see why both exist] → Documented in `docs/viewport.md`: `auto` is the default and may grow with the two panes.

## Migration Plan

Additive. An older studio never sends `camera` and sees `auto`. The studio checks `hello.protocol` ≥ 1.25 before showing the toggle. Rollback is reverting the commit.

## Open Questions

None.
