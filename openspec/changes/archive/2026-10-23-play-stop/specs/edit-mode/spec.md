## MODIFIED Requirements

### Requirement: Play and edit modes
`bevy-ts/viewport_mode` (`mode`: `play` or `edit`, optional `ui`, optional `gamepadUnfocused`) SHALL switch modes and answer the mode in force, with `started`: whether the game's TS `Startup` systems have run in this world. With no `mode` it SHALL answer without switching. Entering `edit` SHALL pause as `bevy-ts/pause` does, deactivate every game camera (remembering which were active), activate the editor camera on the virtual window, and enable the editor tools; entering it from `play` SHALL also be a Stop (see "Stop rebuilds the edit world from source"). Entering `play` SHALL restore the game cameras' active flags, deactivate the editor camera and tools, run the game's `Startup` systems if they have not run in this world, and resume unless the game was paused before edit was entered. `bevy-ts/step` SHALL work in edit mode and render the stepped frame through the editor camera; before the game has started it SHALL run `Startup` first and leave the mode as it was. A reload SHALL keep the mode.

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

## ADDED Requirements

### Requirement: Stop rebuilds the edit world from source
A switch from `play` to `edit` (a Stop), paused or not, SHALL throw the play world away and rebuild it from the live generation and the files on disk, as a `restart` reload does, without recompiling: it SHALL re-apply the live generation's module set. The engine SHALL keep no copy of the world. After a Stop:
- every entity TS spawned, every TSX node and every tombstone SHALL be gone, and every scene SHALL mount as if first;
- which `Startup` systems ran SHALL be forgotten and `started` SHALL be false;
- `Local` storage, TS resources, TS states and pending TS messages SHALL be reset as a `restart` resets them (hot-reload);
- the JS engine SHALL use a fresh instance;
- `Time<Virtual>`, `Time<Fixed>` and the script frame count SHALL start again from 0, and stay paused;
- the editor camera's pose, the gizmo tools, `ui` and `gamepadUnfocused` SHALL be kept.

A hot reload applied during play SHALL be what Stop rebuilds. The `viewport_mode` answer SHALL come after the rebuild and the levels' reconcile, and `tsx_tree+watch` and `level+watch` SHALL report the remount as they report a restart. A `restart` reload during play SHALL NOT be a Stop: it SHALL keep `play` and run `Startup` again.

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

### Requirement: Levels and the selection come back after Stop
A Stop SHALL leave open exactly the levels that were open when play was entered, each with the opener it had then: a level a client opened during play SHALL close, and one a client closed during play SHALL open again. A game that booted in play (no `--edit`) entered no play to go back to: its first Stop SHALL keep the levels open at that moment. Each level SHALL be reconciled from disk before Stop answers. The selection SHALL be kept by node id: an item whose node id names an entity after the rebuild SHALL be selected again with that entity, and the change SHALL be streamed with `origin: "engine"`; an item with no node id, or whose node is gone, SHALL be dropped.

#### Scenario: Enemy b stays selected
- **WHEN** enemy `b` of `levels/arena.bsn` is selected, and a client sets `play` then `edit`
- **THEN** after the Stop enemy `b`'s new entity is selected, and `viewport_selection+watch` sends it

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
`bevy-ts/viewport_mode+watch` SHALL send the mode as `viewport_mode` answers it (`mode`, `ui`, `gamepadUnfocused`, `paused`, `started`) plus `cause`, on open with `cause: "engine"` and on every change of any of those fields:
- `client`: a `viewport_mode`, `pause` or `step` call changed it;
- `restart`: a `restart` reload changed it;
- `failed`: a Play was refused.

A Stop SHALL send one message, after its rebuild. A new engine process (project-engine) SHALL start a new stream; a client SHALL resubscribe after it reconnects.

#### Scenario: The studio follows Play and Stop
- **WHEN** the studio subscribes on an `--edit` engine, then a client sets `play`, then `edit`
- **THEN** the stream sends `edit`, `started: false`, `cause: "engine"`; then `play`, `started: true`, `cause: "client"`; then `edit`, `started: false`, `cause: "client"`

#### Scenario: A step starts the game
- **WHEN** an `--edit` engine has not played and a client calls `bevy-ts/step`
- **THEN** the stream sends `mode: "edit"`, `started: true`, `paused: true`, `cause: "client"`
