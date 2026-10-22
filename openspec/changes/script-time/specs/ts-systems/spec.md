## MODIFIED Requirements

### Requirement: Time as a resource view
`Res<Time>`, `Res<Time<Real>>`, `Res<Time<Virtual>>` and `Res<Time<Fixed>>` SHALL each give a read-only view of that Bevy clock, built per run from the world with no reflection, with Bevy's getter names as snake_case properties and every `Duration` as seconds (`number`). `bevy-ts` SHALL export a generic `Time<C>` and the marker types `Real`, `Virtual` and `Fixed`; `Time` alone is `Time<()>`, as in Bevy.

Every clock SHALL have `delta_secs`, `elapsed_secs` and `elapsed_secs_wrapped` (`f32`), and `delta_secs_f64`, `elapsed_secs_f64`, `elapsed_secs_wrapped_f64` and `wrap_period` (`f64`). `Time<Virtual>` SHALL add `is_paused`, `was_paused`, `relative_speed`, `relative_speed_f64`, `effective_speed`, `effective_speed_f64` and `max_delta`. `Time<Fixed>` SHALL add `timestep`, `overstep`, `overstep_fraction` and `overstep_fraction_f64`. `Time<Real>`'s `Instant`s SHALL NOT be visible.

`Res<Time>` SHALL read Bevy's generic `Time` for the schedule the system runs in: `Time<Virtual>`'s values in `Update`, `Time<Fixed>`'s in `FixedUpdate` and every fixed schedule. Any resource whose path is `bevy_time::time::Time` with or without a context (`Time<Real>`, `bevy_time::time::Time<bevy_time::fixed::Fixed>`) SHALL route to this view and to no other resource path. `ResMut` of any clock SHALL fail the load with its location, until clock writes land.

#### Scenario: Fixed step
- **WHEN** a `FixedUpdate` system reads `time.delta_secs` from `Res<Time>` with the timestep at 1/64 s
- **THEN** it reads 1/64 on every run, not the frame's delta

#### Scenario: The game clock's state
- **WHEN** Rust sets `Time<Virtual>`'s relative speed to 0.5 and a TS system reads `Res<Time<Virtual>>`
- **THEN** it reads `relative_speed` 0.5 and `is_paused` false, and `Res<Time<Real>>`'s `delta_secs` is about twice `Res<Time>`'s

#### Scenario: Fixed-step reads
- **WHEN** an `Update` system reads `Res<Time<Fixed>>`
- **THEN** `timestep` is Bevy's timestep in seconds and `overstep_fraction` is in `[0, 1)`

#### Scenario: Writing a clock
- **WHEN** a system takes `ResMut<Time<Virtual>>`
- **THEN** loading fails with the file and line, saying clocks are read-only from TS

## ADDED Requirements

### Requirement: The editor clock leaves a game's pause alone
bevy-ts's editor clock (`bevy-ts/pause`, `bevy-ts/step`, edit mode, a Stop) SHALL record whether it paused `Time<Virtual>` itself, and SHALL unpause `Time<Virtual>` only to undo its own pause. A pause of `Time<Virtual>` by the game, from Rust or TS, SHALL hold until the game unpauses it. A frame the editor steps while the game has paused `Time<Virtual>` SHALL run the bevy-ts stages and advance zero virtual time.

#### Scenario: A game pause survives the next frame
- **WHEN** a Rust system calls `Time<Virtual>::pause()` in a running game
- **THEN** `Time<Virtual>` is still paused on the next 10 frames, and `Res<Time>`'s `delta_secs` is 0 on each

#### Scenario: Editor pause over a game pause
- **WHEN** the game has paused `Time<Virtual>`, the client pauses with `bevy-ts/pause`, then resumes
- **THEN** the bevy-ts stages run again and `Time<Virtual>` stays paused
