## 1. The choice and the view (`crates/bevy-ts-viewport/src/edit.rs`)

- [x] 1.1 `CameraChoice { Auto, Editor, Game }` on `EditState` (`camera_choice`, default `Auto`); rename the editor camera field `camera` to `editor_camera`; `EditState::editor_view()`: mode Edit and choice not `Game`
- [x] 1.2 `apply_view(world)`: through `set_active`, the editor view turns the editor and overlay cameras on and game cameras off; the game view turns the editor and overlay cameras off and each game camera to its remembered flag; `EditorGizmos` on only in the editor view; `TransformGizmoFocus` removed on entering the game view
- [x] 1.3 `enter_edit` records `game_cameras` and spawns the editor camera as today, then `apply_view`; `boot_edit` and Stop keep the choice, so a Stop with `Game` comes back to the game view over the rebuilt world
- [x] 1.4 `camera_added` in the game view records the flag and leaves the camera as it came
- [x] 1.5 No game camera: check whether Bevy 0.20 clears a camera-less window target; if not, an `EditorOnly` clear-only camera (`order: -11`, empty `RenderLayers`) on in the game view while no game camera is on. It does (`handle_uncovered_swap_chains`): no clear camera
- [x] 1.6 `apply_ui`: in the game view neither `UiSink` nor the editor camera holds `IsDefaultUiCamera`
- [x] 1.7 Gate the tool systems on `editor_view()` instead of `mode == Edit`: the `edit.rs` Update chain, `edits.rs` PostUpdate, `orient.rs` (hidden in the game view)
- [x] 1.8 `game_cameras_count(world)`: game cameras alive with their flag on (remembered in edit, `is_active` with `Pending` in Play)

## 2. The call and the watch

- [x] 2.1 `set_mode` takes `camera`: refuses `editor` when the resulting mode is Play (-32602, "The editor camera is not available in Play.", `ModeCause::Failed`, nothing changed); a `camera`-only change in edit runs `apply_view` and no Stop
- [x] 2.2 `mode_json` answers `camera` and `gameCameras`
- [x] 2.3 `watch_mode`: when only `gameCameras` differs and no restart ran, cause `cameras`
- [x] 2.4 `viewport_pick` in the game view answers -32602 "Picking needs the editor camera."; a click in the game view selects nothing

## 3. Protocol

- [x] 3.1 `protocol/bevy-ts.openrpc.json`: `ViewportCameraChoice`; `camera` on `ViewportModeParams`; `camera` and `gameCameras` optional on `ViewportModeResult`, required on `ViewportModeWatch`; `cameras` in `ViewportModeCause`; the next free minor on main at landing (1.25.0 today), told to lead@bevy-ts; regenerated Rust and TS types; CI diff check

## 4. Tests (`crates/bevy-ts-viewport/tests/edit.rs`, one per scenario)

- [x] 4.1 A 1.24 client: no `camera` sent, every answer `camera: "auto"`, views as before
- [x] 4.2 Looking through the arena's camera: game view pixels match a Play frame at frame 0 (no grid, outline or handle), `gameCameras: 1`, `paused`, not started; back to editor at the same pose, selection kept
- [x] 4.3 No picking in the game view: a click keeps the selection; `viewport_pick` errors
- [x] 4.4 Stepping in the game view renders through the game camera
- [x] 4.5 Play and Stop keep the choice: `game`, play, edit comes back to the game view after the rebuild
- [x] 4.6 The editor camera in Play refused, both forms; `cause: "failed"`; nothing changed
- [x] 4.7 Two cameras draw in order (a minimap viewport at `order: 1`); `gameCameras: 2`
- [x] 4.8 A game with no camera yet: `gameCameras: 0`, the view is the clear colour with no stale editor frame; a step makes it 1
- [x] 4.9 A camera that starts off is not counted and does not draw
- [x] 4.10 The watch: a level gains a camera through `level_write`, `cause: "cameras"`; the arena HUD draws in the game view as in Play (`crates/bevy-ts-cli/tests/play_stop.rs`, a real game with a level)

## 5. Docs and gate

- [x] 5.1 `docs/viewport.md`: the game view (what it shows, what is off, `auto` vs `editor`, Play and Stop, `gameCameras` and "No cameras rendering"), the `cameras` cause, and for the studio the minor to check
- [ ] 5.2 Workspace gate once, last: build, tests (viewport's render tests one at a time), clippy
