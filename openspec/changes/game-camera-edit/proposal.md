## Why

In edit mode the studio sees the scene only from the editor camera. To check a shot it has to Play, which starts the game and throws the world away on Stop. Unity's Game view shows what the game cameras see while the scene stays paused. play-stop left this out (its "game camera while editing"); this change adds it, as a swap of the one view, so nothing renders twice.

## What Changes

- **`viewport_mode` gains `camera`: `"auto" | "editor" | "game"`**, in its params, its result and `viewport_mode+watch`. `"auto"` is the default and keeps today's behaviour, so a 1.24 client sees no change.
- **`camera: "game"` in edit mode** turns the editor camera, grid, outlines, transform handles and orientation gizmo off, and the game cameras on. The world stays paused. No click selects, no gizmo drags, no edit key acts, and `viewport_pick` is refused. `bevy-ts/step` renders the stepped frame through the game cameras.
- **Play always shows the game cameras.** `camera: "editor"` in a call that is in, or enters, Play is refused and changes nothing. The engine keeps the choice: Stop brings back the camera chosen before Play.
- **The result carries `gameCameras: n`**, the number of game cameras Play would turn on (in Play, the number on). Under `--edit` before the first Play it counts only level and scene cameras, since `Startup` has not run.
- **With no game camera** the view clears to the game's clear colour. The studio shows "No cameras rendering" from `gameCameras: 0`.
- **With several game cameras** all draw, in their `order`, as Play draws them.
- **`viewport_mode+watch`** sends `camera` and `gameCameras`, and a new cause, `cameras`, when only the game camera count changed (a reload, a level edit, a game system).
- **Protocol:** additions only, the next free minor on main at landing (1.25.0 today).
- **Out of scope:** the two-pane split (editor and game views side by side, a second render target and surface). Picking or editing from the game view.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `edit-mode`: `viewport_mode` takes and answers `camera` and answers `gameCameras`; edit mode can show the game cameras with the tools off; the mode watch streams both.

## Impact

- `crates/bevy-ts-viewport`: `edit.rs` (`EditState` keeps the choice; `enter_edit`, `enter_play`, `camera_added` and a new view swap apply on the next `First`; a clear-only camera for no game camera; UI ownership), the tool systems in `edit.rs`, `edits.rs` and `orient.rs` gated on the editor view, `methods.rs` (`viewport_mode` params, the refusal, `viewport_pick` refused in the game view), the watch's `cameras` cause.
- `protocol/bevy-ts.openrpc.json`: `ViewportCamera`, `camera` on `ViewportModeParams`, `ViewportModeResult` and `ViewportModeWatch`, `gameCameras`, the `cameras` cause; the next free minor; regenerated Rust and TS types.
- `docs/viewport.md`: the game view, for the studio.
- abyme: the studio adds a game-camera toggle and the "No cameras rendering" message (lead@studio-play-stop). An engine older than this minor has no game view, so the studio checks `hello.protocol` before it sends `camera`.
- Needs the operator's go: nothing. It swaps the one view's cameras, so nothing renders twice and the cost is a game's own Play cost. No new dependency, infrastructure or pattern; the two-pane split, which would need a second surface, is left out.
