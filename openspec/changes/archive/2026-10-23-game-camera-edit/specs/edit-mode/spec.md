## MODIFIED Requirements

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

## ADDED Requirements

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
