# edit-mode Specification

## Purpose
Edit mode: the same engine and world, paused, seen from an editor camera with engine-drawn gizmos, picking that selects what the source placed, and a selection synced with the editor, in a viewport or the engine's own window.

## Requirements

### Requirement: Play and edit modes
`bevy-ts/viewport_mode` (`mode`: `play` or `edit`, optional `ui`, optional `gamepadUnfocused`) SHALL switch modes and answer the mode in force. Entering `edit` SHALL pause as `bevy-ts/pause` does, deactivate every game camera (remembering which were active), activate the editor camera on the virtual window, and enable the editor tools. Entering `play` SHALL restore the game cameras' active flags, deactivate the editor camera and tools, and resume unless the game was paused before edit was entered. `bevy-ts/step` SHALL work in edit mode and render the stepped frame through the editor camera. A reload SHALL keep the mode.

#### Scenario: Toggling to edit and back
- **WHEN** the cube is bobbing and a client sets `edit`, waits two seconds, then sets `play`
- **THEN** in edit the cube stands still and is seen from the same pose the game camera had; in play it bobs from where it stopped and the game camera renders again

#### Scenario: Stepping in edit mode
- **WHEN** in edit mode a client calls `bevy-ts/step` with `frames: 1`
- **THEN** the cube moves by one frame's bob and the viewport shows it from the editor camera

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
