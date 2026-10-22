## ADDED Requirements

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

## MODIFIED Requirements

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
