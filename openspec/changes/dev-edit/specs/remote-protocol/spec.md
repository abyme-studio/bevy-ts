## MODIFIED Requirements

### Requirement: Pausing for editing
`bevy-ts/pause` (`paused: boolean`) SHALL stop or resume all bevy-ts stages and `Time<Virtual>`, so an edited value is not overwritten by a system on the next frame. `bevy-ts/step` (`frames: number`, default 1) SHALL, while paused, run that many frames and pause again. The window and rendering SHALL keep running while paused. Before the game has started (see edit-mode), `bevy-ts/pause` with `paused: false` SHALL fail with "the game has not started; switch to play", and leave it paused.

#### Scenario: Editing a paused game
- **WHEN** the game is paused and the inspector sets the cube's `translation.y` to 3
- **THEN** the cube stays at 3 until the game resumes

#### Scenario: Stepping one frame while paused
- **WHEN** the game is paused and the client calls `bevy-ts/step` with `frames: 1`
- **THEN** every stage and `Time<Virtual>` advance by exactly one frame, and the game is paused again

#### Scenario: Resuming a game that never started
- **WHEN** an `--edit` engine has not played and a client calls `bevy-ts/pause` with `paused: false`
- **THEN** the call fails with "the game has not started; switch to play" and no TS system runs
