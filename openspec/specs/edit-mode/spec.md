# edit-mode Specification

## Purpose
Edit mode: the same engine and world, paused, seen from an editor camera with engine-drawn gizmos, picking that selects what the source placed, and a selection synced with the editor, in a viewport or the engine's own window.

## Requirements

### Requirement: Play and edit modes
`bevy-ts/viewport_mode` (`mode`: `play` or `edit`, optional `ui`, optional `gamepadUnfocused`) SHALL switch modes and answer the mode in force, with `started`: whether the game's TS `Startup` systems have run in this world. With no `mode` it SHALL answer without switching. Entering `edit` SHALL pause as `bevy-ts/pause` does, deactivate every game camera (remembering which were active), activate the editor camera on the virtual window, and enable the editor tools. Entering `play` SHALL restore the game cameras' active flags, deactivate the editor camera and tools, run the game's `Startup` systems if they have not run in this world, and resume unless the game was paused before edit was entered. `bevy-ts/step` SHALL work in edit mode and render the stepped frame through the editor camera; before the game has started it SHALL run `Startup` first and leave the mode as it was. A reload SHALL keep the mode.

#### Scenario: Toggling to edit and back
- **WHEN** the cube is bobbing and a client sets `edit`, waits two seconds, then sets `play`
- **THEN** in edit the cube stands still and is seen from the same pose the game camera had; in play it bobs from where it stopped and the game camera renders again

#### Scenario: Stepping in edit mode
- **WHEN** in edit mode a client calls `bevy-ts/step` with `frames: 1`
- **THEN** the cube moves by one frame's bob and the viewport shows it from the editor camera

#### Scenario: The first Play
- **WHEN** the arena was started with `--edit` and a client sets `play`
- **THEN** the arena's `Startup` systems run once, the enemies start moving, and the answer has `mode: "play"` and `started: true`

#### Scenario: Back to edit before E4
- **WHEN** after the first Play a client sets `edit`, then `play` again
- **THEN** the world stays as play left it, `started` stays true, and `Startup` does not run again

#### Scenario: Stepping before the game started
- **WHEN** an `--edit` engine has not played and a client calls `bevy-ts/step` with `frames: 1`
- **THEN** `Startup` runs, one frame runs, the mode is still `edit` and `started` is true

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
`bevy_ts::EngineMode` SHALL be a Bevy `States` with `Play` and `Edit`, defaulting to `Play`, present in every app with the bevy-ts plugin. It SHALL be `Edit` from the first frame under `--edit`, and SHALL follow every switch made by `bevy-ts/viewport_mode`, applied in that frame's `StateTransition` so `OnEnter` and `OnExit` run before the next `PreUpdate`. A shipped game SHALL always be in `Play`. A native system that must not run in edit mode SHALL be gated with `run_if(in_state(EngineMode::Play))`; bevy-ts SHALL NOT gate Rust systems it did not add.

#### Scenario: A native spinner
- **WHEN** a native crate adds `spin.run_if(in_state(EngineMode::Play))` to `Update` and the engine starts with `--edit`
- **THEN** the spinner does not turn until a client sets `play`, and stops turning when it sets `edit`

#### Scenario: A shipped game
- **WHEN** `bevy-ts play` runs a built game
- **THEN** `EngineMode` is `Play` from the first frame

### Requirement: Edit mode runs no game code
While in edit mode, a TS trigger or lifecycle event that would run a TS observer SHALL be dropped, not queued for play. An edit made in edit mode SHALL therefore fire no TS observer when play resumes.

#### Scenario: Dragging an enemy in edit
- **WHEN** a TS observer watches `Insert<Transform>` on enemies, and in edit mode the operator drags enemy `b` and then sets `play`
- **THEN** the observer does not run for the drag, before or after play
