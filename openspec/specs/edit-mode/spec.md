# edit-mode Specification

## Purpose
Edit mode: the same engine and world, paused, seen from an editor camera with engine-drawn gizmos, picking that selects what the source placed, and a selection synced with the editor, in a viewport or the engine's own window.

## Requirements

### Requirement: Play and edit modes
`bevy-ts/viewport_mode` (`mode`: `play` or `edit`, optional `ui`, optional `gamepadUnfocused`, optional `camera`) SHALL switch modes and answer the mode in force, with `started`: whether the game's TS `Startup` systems have run in this world, `camera`: the camera choice in force, and `gameCameras`: the number of game cameras Play would turn on (see "The game view in edit mode"). With no `mode` it SHALL answer without switching. Entering `edit` SHALL pause as `bevy-ts/pause` does, deactivate every game camera (remembering which were active), activate the editor camera on the virtual window, and enable the editor tools, unless the camera choice is `game`; entering it from `play` SHALL also be a Stop (see "Stop rebuilds the edit world from source"). Entering `play` SHALL restore the game cameras' active flags, deactivate the editor camera and tools, run the game's `Startup` systems if they have not run in this world, and resume unless the game was paused before edit was entered. `bevy-ts/step` SHALL work in edit mode and render the stepped frame through the camera in view; before the game has started it SHALL run `Startup` first and leave the mode as it was. A reload SHALL keep the mode and the camera choice.

#### Scenario: Toggling to edit and back
- **WHEN** the cube is bobbing and a client sets `edit`, waits two seconds, then sets `play`
- **THEN** in edit the cube stands at its scene position and is seen from the same pose the game camera had; in play it bobs from the start again and the game camera renders again

#### Scenario: Stepping in edit mode
- **WHEN** in edit mode a client calls `bevy-ts/step` with `frames: 1`
- **THEN** the cube moves by one frame's bob and the viewport shows it from the editor camera

#### Scenario: The first Play
- **WHEN** the arena was started with `--edit` and a client sets `play`
- **THEN** the arena's `Startup` systems run once, the enemies start moving, and the answer has `mode: "play"` and `started: true`

#### Scenario: Back to edit before E4
- **WHEN** after the first Play a client sets `edit`, then `play` again
- **THEN** the edit answer has `started: false`, and the second Play runs the arena's `Startup` systems once more

#### Scenario: Stepping before the game started
- **WHEN** an `--edit` engine has not played and a client calls `bevy-ts/step` with `frames: 1`
- **THEN** `Startup` runs, one frame runs, the mode is still `edit` and `started` is true

#### Scenario: A 1.24 client
- **WHEN** a client that never sends `camera` switches between `play` and `edit`
- **THEN** every answer has `camera: "auto"`, and the views are the ones above

### Requirement: Editor-only entities are invisible to the game and the protocol
The editor camera, gizmo meshes and any other entity the edit tools spawn SHALL carry an `EditorOnly` marker, SHALL render only on render layer 31 (which the editor camera alone sees in addition to the game's layers), SHALL be absent from `bevy-ts/tsx_tree`, from `bevy-ts/entities+watch` and from `.bsn` writing, and SHALL NOT match TS system queries.

#### Scenario: Listing entities in edit mode
- **WHEN** a client lists entities through `bevy-ts/entities+watch` while in edit mode
- **THEN** the editor camera and gizmo entities are not in it

### Requirement: The editor camera
The editor camera SHALL start, on first entering edit mode, at the pose and projection of the game camera that was rendering to the primary window, and keep its own pose across later mode switches in the same process. It SHALL be driven by viewport input: right button held with `W A S D Q E` to fly and the pointer to look, middle button or ⌥ with left button to orbit around the focus point, wheel to dolly, pinch to dolly, and `F` to frame the selection. `bevy-ts/viewport_camera` SHALL get and set its pose (`translation`, `rotation`, `fov` or orthographic `scale`).

#### Scenario: Framing the selection
- **WHEN** enemy `b` is selected and the viewer sends `F`
- **THEN** the editor camera moves so enemy `b`'s bounds fill the middle of the view, keeping its direction

### Requirement: Engine-drawn gizmos
In edit mode the engine SHALL draw, with `bevy_gizmos` on layer 31: a ground grid, an outline (the world-space bounds) of each selected entity and of the hovered one, light and camera icons for entities with no mesh, and Bevy 0.20's `TransformGizmoPlugin` handles on the primary selection. `bevy-ts/viewport_mode` with `ui: true` SHALL also draw the game's UI over the editor view; the default SHALL NOT.

#### Scenario: Selecting a light
- **WHEN** the operator clicks a point light's icon in edit mode
- **THEN** the light is selected and its outline and transform handles are drawn

### Requirement: Picking selects what the source placed
In edit mode game pointer input SHALL be off (`PointerInputSettings` mouse disabled) and no picking pointer SHALL exist, so game `Pointer<…>` observers and TSX handlers do not fire. A click with no drag SHALL pick by a ray cast from the editor camera against meshes and against light and camera icons, and select, from the topmost hit, the nearest ancestor that is a prefab instance root (an entity whose TSX source chain has a call site, or a `.bsn` level instance); ⌥-click SHALL select the hit entity itself; ⇧-click SHALL add to or remove from the selection; a click on nothing SHALL clear it. `bevy-ts/viewport_pick` (`x`, `y`) SHALL answer every hit at that point, nearest first, with entity, depth, its world `point` `[x, y, z]` and the entity the click would select, and a `ground` `[x, y, z]`, where the ray meets the plane y = 0 in front of the camera, or null when it does not, without changing the selection.

#### Scenario: Clicking an enemy's eye
- **WHEN** the operator clicks enemy `b`'s `eye` child mesh
- **THEN** enemy `b` (the `<Enemy key="b">` instance root) is selected; with ⌥ held, the `eye` entity is

#### Scenario: A button in the HUD during edit
- **WHEN** the operator clicks the HUD's Spawn button in edit mode with `ui: true`
- **THEN** the button's `<button>` entity is selected and no enemy is spawned

#### Scenario: Where a dropped model lands
- **WHEN** the studio calls `viewport_pick` at a point over empty floor below the horizon, then at a point on enemy `b`
- **THEN** the first answers no hits and a `ground` with y = 0; the second's first hit has a `point` on `b`'s surface

#### Scenario: Pointing at the sky
- **WHEN** the studio calls `viewport_pick` at a point above the horizon with nothing there
- **THEN** it answers no hits and `ground: null`

### Requirement: Selection is synced with the editor
`bevy-ts/viewport_selection+watch` SHALL send the selection on open and on every change, from a click or from `bevy-ts/viewport_select`. Each selected item SHALL carry the entity (`Entity::to_bits`), its `Name`, its TSX node id and source chain when it has a `TsxNode` (as `bevy-ts/tsx_node` gives it), its `.bsn` level path and instance when a level placed it, and `primary: true` on the one the gizmo acts on. `bevy-ts/viewport_select` (`entities`, or `file`, `line`, `column` resolved as `bevy-ts/tsx_find` resolves them) SHALL set the selection, and the change SHALL be streamed back with `origin: "editor"`; a click's change SHALL have `origin: "viewport"`.

#### Scenario: Selecting from the outliner
- **WHEN** the editor calls `bevy-ts/viewport_select` with the `<Enemy key="b">` line of `game.tsx`
- **THEN** enemy `b` is outlined in the viewport and the watch stream sends it with `origin: "editor"`

#### Scenario: Selecting in the viewport
- **WHEN** the operator clicks enemy `a`
- **THEN** the watch stream sends enemy `a` with its source chain ending at `game.tsx`'s `<Enemy key="a">` line and `origin: "viewport"`

### Requirement: Edit-mode keys stay the engine's
In edit mode the engine SHALL handle `W`, `E`, `R` (transform gizmo translate, rotate, scale), `X` (toggle local and world space), `F`, `Escape` (clear selection) and the camera keys while the hole has focus, and SHALL NOT pass them to the paused game. Which keys the viewer forwards at all is the viewer's choice.

#### Scenario: Switching to rotate
- **WHEN** an entity is selected and the viewer sends `KeyE`
- **THEN** the transform gizmo shows rotation rings

### Requirement: Gizmo tools over the protocol
`bevy-ts/viewport_tools` (`mode?`: `translate`, `rotate` or `scale`; `space?`: `world` or `local`; `snap?`: boolean) SHALL set the transform gizmo's mode, space and snap and answer `ViewportTools`: `mode`, `space`, `snap`, and `steps` (`translate: 0.5`, `rotate: 15` in degrees, `scale: 0.1`). With no params it SHALL answer the tools unchanged. With `snap: true` a drag SHALL move, turn or scale in whole steps of the mode's step; with `snap: false` it SHALL not snap. The tools SHALL start as `translate`, `world`, snap off, and SHALL be kept for the process across mode switches and reloads. The method SHALL be served in play too; the tools act in edit mode. `bevy-ts/viewport_tools+watch` SHALL send `ViewportTools` on open and on every change, with `origin: "editor"` for a change by the method and `origin: "viewport"` for one by the `W`, `E`, `R` or `X` key. No key SHALL toggle snap.

#### Scenario: Picking rotate from the toolbar
- **WHEN** enemy `b` is selected in edit mode and the editor calls `bevy-ts/viewport_tools` with `mode: "rotate"`
- **THEN** the gizmo shows rotation rings, the answer has `mode: "rotate"`, and the watch stream sends it with `origin: "editor"`

#### Scenario: A key changes the strip
- **WHEN** the viewer sends `KeyX` with the tools in `world` space
- **THEN** the watch stream sends `space: "local"` with `origin: "viewport"`

#### Scenario: Snapped move
- **WHEN** snap is on and the operator drags enemy `b`'s x handle from 3 by 1.4 units
- **THEN** enemy `b`'s x is 4.5 during the drag and in the edit on release

### Requirement: Orientation gizmo
In edit mode the engine SHALL draw, in the game viewport only, an orientation gizmo in the top-left corner: a 96 logical px square inset 8 px from the top and left edges, showing the world's axes as the editor camera sees them, with `+X` red, `+Y` green and `+Z` blue as filled ends and the negative axes as hollow ends. It SHALL NOT be drawn in play, in asset previews or in the engine's own window. A click with no drag on an axis end SHALL turn the editor camera to look down that axis at its focus point, keeping its distance from the focus; for `±Y` it SHALL stop at the pitch limit orbiting has (1.54 rad), so the next orbit does not jump. The gizmo SHALL be redrawn only on a frame where the editor camera turned or the viewport resized. A press in the square SHALL NOT pick, change the selection or start a gizmo drag.

#### Scenario: Looking down from above
- **WHEN** in edit mode the operator clicks the gizmo's `+Y` end
- **THEN** the editor camera looks down at its focus point, to the pitch limit, from the same distance, and the selection is unchanged

#### Scenario: Play hides it
- **WHEN** the client switches to `play`
- **THEN** no orientation gizmo is drawn and a click in the top-left corner reaches the game

### Requirement: Starting in edit mode
`bevy-ts dev --edit <entry>` SHALL start the engine in edit mode, with or without `--viewport`; `--edit` with `--headless` SHALL fail with a message saying they cannot be combined. The first generation SHALL load and register its components, resources, systems, observers and levels, and the levels named with `app.level()` SHALL load and reconcile, so the scene shows from the editor camera. Before the first frame, `Time<Virtual>` and every bevy-ts stage SHALL be paused, the game cameras inactive, and the editor camera active at the pose of the first active game camera on the primary window once one exists. No TS `Startup`, `PreUpdate`, `Update`, `PostUpdate` or `FixedUpdate` system and no TS observer SHALL run. Scene functions and bindings SHALL run, because they build the scene. A new engine process after a Rust rebuild (project-engine) SHALL come back in edit mode, whatever mode was in force, as Unity's "Stop Playing and Recompile". A `restart` reload SHALL keep the mode; in edit mode it SHALL leave the game not started.

#### Scenario: Opening the arena
- **WHEN** abyme starts `bevy-ts dev --viewport --edit examples/arena/src/game.tsx` and attaches
- **THEN** the arena's light and enemies are in the viewport from the editor camera, no enemy has moved from its level transform, `viewport_mode {}` answers `mode: "edit"` and `started: false`, and `bevy-ts/systems` shows no TS system with a last run time

#### Scenario: The studio switches to edit after hello
- **WHEN** the studio calls `viewport_mode { mode: "edit" }` on an `--edit` engine that has not played
- **THEN** nothing changes: no `Startup` or TS system runs, the editor camera keeps its pose, and the answer has `mode: "edit"` and `started: false`

#### Scenario: Started by hand with a window
- **WHEN** a developer runs `bevy-ts dev --edit examples/cube/src/game.ts`
- **THEN** the engine's own window shows the cube standing still from the editor camera

#### Scenario: Headless edit
- **WHEN** a developer runs `bevy-ts dev --edit --headless examples/cube/src/game.ts`
- **THEN** it exits with code 2 and "--edit and --headless cannot be combined"

#### Scenario: A Rust edit while playing
- **WHEN** an `--edit` engine on a project engine is in play and a save in `native/src/lib.rs` compiles
- **THEN** the new engine comes up in edit mode with `started: false`

### Requirement: The engine mode is a Bevy state
`bevy_ts::EngineMode` SHALL be a Bevy `States` with `Play` and `Edit`, defaulting to `Play`, present in every app with the bevy-ts plugin. It SHALL be `Edit` from the first frame under `--edit`, and SHALL follow every switch made by `bevy-ts/viewport_mode`, applied in that frame's `StateTransition` so `OnEnter` and `OnExit` run before the next `PreUpdate`. A shipped game SHALL always be in `Play`. A native system that must not run in edit mode SHALL be gated with `run_if(in_state(EngineMode::Play))`; bevy-ts SHALL NOT gate Rust systems it did not add. On a Stop, `OnExit(EngineMode::Play)` SHALL run before bevy-ts rebuilds the TS world, so a native crate resets its own resources and despawns the entities it spawned there; bevy-ts SHALL NOT reset or despawn what Rust spawned.

#### Scenario: A native spinner
- **WHEN** a native crate adds `spin.run_if(in_state(EngineMode::Play))` to `Update` and the engine starts with `--edit`
- **THEN** the spinner does not turn until a client sets `play`, and stops turning when it sets `edit`

#### Scenario: A native counter resets itself
- **WHEN** a native crate counts frames in a resource during play, resets it in `OnExit(EngineMode::Play)`, and a client sets `edit` after 100 frames
- **THEN** the counter is 0 when the TS rebuild runs, and an entity a Rust system spawned without `OnExit` cleanup is still in the world

#### Scenario: A shipped game
- **WHEN** `bevy-ts play` runs a built game
- **THEN** `EngineMode` is `Play` from the first frame

### Requirement: Edit mode runs no game code
While in edit mode, a TS trigger or lifecycle event that would run a TS observer SHALL be dropped, not queued for play. An edit made in edit mode SHALL therefore fire no TS observer when play resumes. No script timer SHALL fire and no socket event SHALL be delivered in edit mode, and leaving play SHALL close the game's connections; script-networking says how.

#### Scenario: Dragging an enemy in edit
- **WHEN** a TS observer watches `Insert<Transform>` on enemies, and in edit mode the operator drags enemy `b` and then sets `play`
- **THEN** the observer does not run for the drag, before or after play

#### Scenario: A timer set before edit
- **WHEN** a playing script sets `setTimeout(f, 100)` and a client sets `edit` at once, waits one second, then sets `play`
- **THEN** `f` does not run in edit mode and runs at the first frame of play

### Requirement: Stop rebuilds the edit world from source
A switch from `play` to `edit` (a Stop), paused or not, SHALL throw the play world away and rebuild it from the live generation and the files on disk, as a `restart` reload does, without recompiling: it SHALL re-apply the live generation's module set. The engine SHALL keep no copy of the world. After a Stop:
- every entity TS spawned SHALL be gone;
- every TSX node and tombstone of a scene SHALL be gone, and every scene SHALL mount as if first;
- every level SHALL be as its files describe it, every tombstone of it cleared, as "Stop restores the levels the play changed" says;
- which `Startup` systems ran SHALL be forgotten and `started` SHALL be false;
- `Local` storage, TS resources, TS states and pending TS messages SHALL be reset as a `restart` resets them (hot-reload);
- the JS engine SHALL use a fresh instance;
- `Time<Virtual>`, `Time<Fixed>` and the script frame count SHALL start again from 0, and stay paused;
- the editor camera's pose, the gizmo tools, `ui` and `gamepadUnfocused` SHALL be kept.

A hot reload applied during play SHALL be what Stop rebuilds. The `viewport_mode` answer SHALL come after the rebuild and the levels' restore, and `tsx_tree+watch` and `level+watch` SHALL report what the Stop despawned and spawned, as they report a reconcile. A `restart` reload during play SHALL NOT be a Stop: it SHALL keep `play` and run `Startup` again.

#### Scenario: Stopping the arena
- **WHEN** the arena has played for five seconds, an enemy has been destroyed and the score is 3, and a client sets `edit`
- **THEN** every enemy stands at its level transform, the destroyed one is back, `Score` reads its `insertResource` value, and the answer has `mode: "edit"` and `started: false`

#### Scenario: Stopping a paused game
- **WHEN** the game is paused in play and a client sets `edit`
- **THEN** the world is rebuilt as for any Stop

#### Scenario: A save during play
- **WHEN** during play `enemy.tsx`'s scene gains a child mesh and reloads, then a client sets `edit`
- **THEN** each enemy after the Stop has the new child, and no compile ran for the Stop

#### Scenario: The editor camera stays
- **WHEN** the operator flew the editor camera before Play, then a client sets `play` and `edit`
- **THEN** the editor camera is at the pose it had before Play

#### Scenario: Time starts again
- **WHEN** a client sets `edit` after ten seconds of play, then `play`
- **THEN** a `Startup` system reading `time.elapsed_secs` reads 0

#### Scenario: The tree watch follows the remount
- **WHEN** a client watches `tsx_tree+watch` during play, holding each row's entity by node id, and a client sets `edit`
- **THEN** the next message sends each remounted or respawned row under `added` with its new entity, no level entity is left outside the client's tree (none shows as spawned at runtime), a kept row is not sent, and the watch does not start over with a snapshot

### Requirement: Levels and the selection come back after Stop
A Stop SHALL leave open exactly the levels that were open when play was entered, each with the opener it had then: a level a client opened during play SHALL close, and one a client closed during play SHALL open again. A game that booted in play (no `--edit`) entered no play to go back to: its first Stop SHALL keep the levels open at that moment. Each level SHALL stand as its files describe it before Stop answers: restored ("Stop restores the levels the play changed") or reconciled from disk. The selection SHALL be kept by node id: an item whose node id names an entity after the rebuild SHALL be selected again with that entity, the same one when the Stop kept it, and the change SHALL be streamed with `origin: "engine"`; an item with no node id, or whose node is gone, SHALL be dropped.

#### Scenario: Enemy b stays selected
- **WHEN** enemy `b` of `levels/arena.bsn` is selected, and a client sets `play` then `edit`
- **THEN** after the Stop enemy `b`'s entity is selected (its new one, if the play changed it), and `viewport_selection+watch` sends it when it changed

#### Scenario: A bullet was selected
- **WHEN** during play a client selects a bullet a TS system spawned, then sets `edit`
- **THEN** the selection is empty after the Stop

#### Scenario: The first Stop of a game booted in play
- **WHEN** an engine started without `--edit` has `levels/arena.bsn` open, a client opens `levels/room.bsn`, then sets `edit`
- **THEN** both levels are open after the Stop, each reconciled from disk

#### Scenario: A level opened during play
- **WHEN** `levels/arena.bsn` was open at Play, and during play a client opens `levels/room.bsn`, then sets `edit`
- **THEN** after the Stop only the arena is open, with the opener it had, and `level+watch` sent the room's `closed`

### Requirement: A Play can fail
`bevy-ts/viewport_mode` with `mode: "play"` SHALL fail, leaving the engine in edit with nothing run, when:
- no generation has been applied yet: `reason: "no-generation"`;
- the newest compile failed, the first included: `reason: "compile-errors"`.

The error SHALL be a JSON-RPC error whose message is a sentence the studio can show as is, naming the first error's file and line (for example `src/game.tsx:3: Unexpected token`), and whose `data` has `reason`. A Play while a compile runs SHALL play the live generation. `bevy-ts/viewport_mode+watch` SHALL send the unchanged mode with `cause: "failed"`.

#### Scenario: A typo before Play
- **WHEN** an `--edit` engine's newest save of `game.tsx` fails to compile at line 3, and a client sets `play`
- **THEN** the call fails with `reason: "compile-errors"` and a message starting `src/game.tsx:3:`, the mode is still `edit`, `started` is false and no TS system has run

#### Scenario: Fixed and played
- **WHEN** the typo is fixed, the save applies, and a client sets `play`
- **THEN** the game plays

### Requirement: The mode is streamed
`bevy-ts/viewport_mode+watch` SHALL send the mode as `viewport_mode` answers it (`mode`, `ui`, `gamepadUnfocused`, `paused`, `started`, `camera`, `gameCameras`) plus `cause`, on open with `cause: "engine"` and on every change of any of those fields:
- `client`: a `viewport_mode`, `pause` or `step` call changed it;
- `restart`: a `restart` reload changed it;
- `failed`: a Play or a camera choice was refused;
- `cameras`: only `gameCameras` changed, because a reload, a level edit or a game system added or removed a game camera.

A Stop SHALL send one message, after its rebuild. A new engine process (project-engine) SHALL start a new stream; a client SHALL resubscribe after it reconnects.

#### Scenario: The studio follows Play and Stop
- **WHEN** the studio subscribes on an `--edit` engine, then a client sets `play`, then `edit`
- **THEN** the stream sends `edit`, `started: false`, `cause: "engine"`; then `play`, `started: true`, `cause: "client"`; then `edit`, `started: false`, `cause: "client"`

#### Scenario: A step starts the game
- **WHEN** an `--edit` engine has not played and a client calls `bevy-ts/step`
- **THEN** the stream sends `mode: "edit"`, `started: true`, `paused: true`, `cause: "client"`

#### Scenario: A level gains a camera
- **WHEN** the studio subscribes on an `--edit` engine whose level has no camera, then a `level_write` adds a camera to that level
- **THEN** the stream sends `gameCameras: 0`, `cause: "engine"`; then `gameCameras: 1`, `cause: "cameras"`

### Requirement: The game view in edit mode
`bevy-ts/viewport_mode` SHALL take `camera`: `auto`, `editor` or `game`, and keep it until a call sets another; it SHALL start as `auto`. In edit mode, `auto` and `editor` SHALL show the editor camera as today, and `game` SHALL show the game view: the editor camera, the transform gizmo's overlay camera, the grid, outlines, icons, transform handles and orientation gizmo off, and every game camera that Play would turn on active, drawing in its `order` as Play draws it, with the game's UI as Play draws it whatever `ui` is. The world SHALL stay paused and run no game code, as edit mode does. In the game view no click SHALL select, no transform handle SHALL drag, no edit key (`F`, the gizmo tool keys) SHALL act, the editor camera SHALL keep its pose, and `bevy-ts/viewport_pick` SHALL answer an error. The selection SHALL be kept, and drawn again when the editor view comes back. Switching between `editor` and `game` in edit mode SHALL be no Stop and change nothing in the world. The swap SHALL apply on the next `First`, as a mode switch does, so no frame shows both views or neither.

In Play the game cameras SHALL always show. A call with `camera: "editor"` that is in, or enters, `play` SHALL be refused with a JSON-RPC error and change nothing; `auto` and `game` SHALL be accepted. A Stop SHALL bring back the camera choice in force before Play.

`gameCameras` SHALL count the game cameras (not `EditorOnly`, not previews) that exist and whose own active flag is on: the ones Play would turn on, or in Play the ones on. Under `--edit` before the first Play it SHALL count only the cameras levels and scene functions spawned, because no `Startup` system has run.

With `gameCameras: 0` in the game view, the view SHALL clear to the game's `ClearColor` and show no stale frame.

#### Scenario: Looking through the arena's camera
- **WHEN** the arena's level has a camera, the arena is in edit mode, not started, enemy `b` selected, and a client sets `camera: "game"`
- **THEN** the next frame shows the arena from that level camera with no grid, outline or handle; the enemies have not moved; the answer has `mode: "edit"`, `camera: "game"`, `paused: true`, `started: false`, `gameCameras: 1`; setting `camera: "editor"` shows the editor camera at its earlier pose with enemy `b` still selected

#### Scenario: No picking in the game view
- **WHEN** the arena is in the game view and the operator clicks enemy `a`, or a client calls `viewport_pick`
- **THEN** the selection is unchanged and `viewport_pick` answers an error

#### Scenario: Stepping in the game view
- **WHEN** the cube is in the game view in edit mode and a client calls `bevy-ts/step` with `frames: 1`
- **THEN** the cube moves by one frame's bob, seen from the game camera

#### Scenario: Play and Stop keep the choice
- **WHEN** a client sets `camera: "game"` in edit mode, then `play`, then `edit`
- **THEN** play shows the game cameras with `camera: "game"`; after Stop the view is the game view again, over the rebuilt world

#### Scenario: The editor camera in Play is refused
- **WHEN** in play a client calls `viewport_mode { camera: "editor" }`, or in edit `viewport_mode { mode: "play", camera: "editor" }`
- **THEN** the call answers an error, the mode and camera choice are unchanged, and `viewport_mode+watch` sends `cause: "failed"`

#### Scenario: Two cameras
- **WHEN** a level has a camera at `order: 0` filling the window and one at `order: 1` drawing a minimap viewport, and a client sets `camera: "game"` in edit mode
- **THEN** both draw, the minimap over the main view, as in Play, and `gameCameras` is 2

#### Scenario: A game with no camera yet
- **WHEN** an `--edit` engine runs a game whose only camera is spawned by a TS `Startup` system, and a client sets `camera: "game"`
- **THEN** `gameCameras` is 0 and the view is the game's clear colour; after `bevy-ts/step` runs `Startup`, `gameCameras` is 1 and the view shows that camera

#### Scenario: A camera that starts off
- **WHEN** a level has two cameras, one with `is_active: false`, and a client sets `camera: "game"`
- **THEN** `gameCameras` is 1 and only the active one draws, as in Play

### Requirement: Stop restores the levels the play changed
A level open when the game started (see scene-reconciler, "The game-start baseline") and still open at the Stop SHALL be kept: its entities the play did not change SHALL stay, with their entity ids and component values, and the Stop SHALL respawn from the level's last resolved tree, without composing or expanding it:
- each subtree whose root entity, or any entity in it, the play changed (a component changed after the baseline, or a component added or removed);
- each node tombstoned while the level was applied, with its subtree.

A level SHALL instead take the full path, remounting as if first, when any of these holds:
- the game started before the level was applied, or there is no baseline (a game booted in play);
- the level was applied again after the baseline (a level reload or a generation applied during play), or it is waiting to be;
- a file it was composed from differs on disk from the text it was composed from;
- it is a scene's sidecar;
- its description holds a JS callback: a binding, a handler, a ref or a `<Show>`/`<For>` slot.

A kept entity SHALL fire no hook or observer and SHALL NOT appear as `Added` to a Rust system. A restored subtree SHALL be stamped (`LevelInstance`, `MissingComponents`) as a reconcile stamps it; the rest SHALL keep their stamps.

#### Scenario: Nothing touched
- **WHEN** `levels/arena.bsn` holds 100 enemies, a client sets `play`, the game reads them and changes none, and a client sets `edit`
- **THEN** every enemy has the entity id it had before Play, and `tsx_tree+watch` reports nothing spawned or despawned

#### Scenario: One enemy moved
- **WHEN** during play a system moves enemy `b` and adds `Stunned` to enemy `c`, then a client sets `edit`
- **THEN** `b` and `c` are respawned with their level values and without `Stunned`, and every other enemy keeps its entity id

#### Scenario: A killed enemy
- **WHEN** during play a system despawns enemy `a`, then a client sets `edit`
- **THEN** `a` exists again with its level values, and `tsx_tree` lists no tombstone

#### Scenario: A stepped game
- **WHEN** an `--edit` engine steps one frame before any Play, which runs `Startup` and moves enemy `b`, then a client sets `play` and `edit`
- **THEN** `b` is back at its level transform

#### Scenario: A level saved during play
- **WHEN** during play `levels/arena.bsn` is saved with enemy `d` added, then a client sets `edit`
- **THEN** the arena remounts as if first and `d` exists

#### Scenario: A level with a handler
- **WHEN** an enemy prefab's scene has an `onClick` handler, and a client sets `play` then `edit`
- **THEN** the arena remounts as if first, and a click on an enemy after the Stop calls the handler of the new isolate

#### Scenario: The selection keeps its entity
- **WHEN** enemy `e` is selected, the play does not change it, and a client sets `edit`
- **THEN** `e` is selected with the same entity, and `viewport_selection+watch` sends no change for it

#### Scenario: A native plugin's Added
- **WHEN** a Rust system counts `Added<Enemy>` and the play changes no enemy, then a client sets `edit` and `play`
- **THEN** the system sees no new `Added<Enemy>` after the Stop
