## MODIFIED Requirements

### Requirement: Time as a resource view
`Res<Time>`, `Res<Time<Real>>`, `Res<Time<Virtual>>` and `Res<Time<Fixed>>` SHALL each give a read-only view of that Bevy clock, built per run from the world with no reflection, with Bevy's getter names as snake_case properties and every `Duration` as seconds (`number`). `bevy-ts` SHALL export a generic `Time<C>` and the marker types `Real`, `Virtual` and `Fixed`; `Time` alone is `Time<()>`, as in Bevy.

Every clock SHALL have `delta_secs`, `elapsed_secs` and `elapsed_secs_wrapped` (`f32`), and `delta_secs_f64`, `elapsed_secs_f64`, `elapsed_secs_wrapped_f64` and `wrap_period` (`f64`). `Time<Virtual>` SHALL add `is_paused`, `was_paused`, `relative_speed`, `relative_speed_f64`, `effective_speed`, `effective_speed_f64` and `max_delta`. `Time<Fixed>` SHALL add `timestep`, `overstep`, `overstep_fraction` and `overstep_fraction_f64`. `Time<Real>`'s `Instant`s SHALL NOT be visible.

`Res<Time>` SHALL read Bevy's generic `Time` for the schedule the system runs in: `Time<Virtual>`'s values in `Update`, `Time<Fixed>`'s in `FixedUpdate` and every fixed schedule. Any resource whose path is `bevy_time::time::Time` with or without a context (`Time<Real>`, `bevy_time::time::Time<bevy_time::fixed::Fixed>`) SHALL route to this view and to no other resource path. `ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>` SHALL write the clock ("Writing the game and fixed clocks"). `ResMut<Time>` and `ResMut<Time<Real>>` SHALL fail the load with its location, naming `ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>` as the clocks TS writes.

#### Scenario: Fixed step
- **WHEN** a `FixedUpdate` system reads `time.delta_secs` from `Res<Time>` with the timestep at 1/64 s
- **THEN** it reads 1/64 on every run, not the frame's delta

#### Scenario: The game clock's state
- **WHEN** Rust sets `Time<Virtual>`'s relative speed to 0.5 and a TS system reads `Res<Time<Virtual>>`
- **THEN** it reads `relative_speed` 0.5 and `is_paused` false, and `Res<Time<Real>>`'s `delta_secs` is about twice `Res<Time>`'s

#### Scenario: Fixed-step reads
- **WHEN** an `Update` system reads `Res<Time<Fixed>>`
- **THEN** `timestep` is Bevy's timestep in seconds and `overstep_fraction` is in `[0, 1)`

#### Scenario: Writing the real clock
- **WHEN** a system takes `ResMut<Time<Real>>`
- **THEN** loading fails with the file and line, naming `ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>` as the clocks TS writes

## ADDED Requirements

### Requirement: The editor clock leaves a game's pause alone
bevy-ts's editor clock (`bevy-ts/pause`, `bevy-ts/step`, edit mode, a Stop) SHALL record whether it paused `Time<Virtual>` itself, and SHALL unpause `Time<Virtual>` only to undo its own pause. A pause of `Time<Virtual>` by the game, from Rust or TS, SHALL hold until the game unpauses it. A frame the editor steps while the game has paused `Time<Virtual>` SHALL run the bevy-ts stages and advance zero virtual time.

#### Scenario: A game pause survives the next frame
- **WHEN** a Rust system calls `Time<Virtual>::pause()` in a running game
- **THEN** `Time<Virtual>` is still paused on the next 10 frames, and `Res<Time>`'s `delta_secs` is 0 on each

#### Scenario: Editor pause over a game pause
- **WHEN** the game has paused `Time<Virtual>`, the client pauses with `bevy-ts/pause`, then resumes
- **THEN** the bevy-ts stages run again and `Time<Virtual>` stays paused

### Requirement: Writing the game and fixed clocks
`ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>` SHALL write Bevy's clock through exports bevy-ts registers in every engine, `VirtualTime` and `FixedTime` in `bevy-ts/time`, claiming those spellings (native-functions, "An export may stand for a Bevy spelling"). Each SHALL have every Bevy setter of its clock in camelCase, every `Duration` as seconds: on `Time<Virtual>` `pause()`, `unpause()`, `toggle()`, `setRelativeSpeed(r)`, `setRelativeSpeedF64(r)`, `setMaxDelta(s)` and `setWrapPeriod(s)`; on `Time<Fixed>` `setTimestep(s)`, `setTimestepSeconds(s)`, `setTimestepHz(hz)`, `discardOverstep(s)` and `setWrapPeriod(s)`. Each SHALL have the clock's getters of "Time as a resource view" as read-only properties under the same names, read live from the resource.

A write SHALL land in the resource when the call returns, at the TS system's tick, and take effect where Bevy reads it: a pause or speed in the next frame's delta, a timestep on the next fixed step. An argument Bevy would panic on (a zero max delta, timestep or wrap period, a negative or non-finite speed or Hz, a negative or non-finite number of seconds) SHALL throw in JS naming the export and the method, with Bevy's message, and leave the clock unchanged. A TS pause SHALL be a game pause under "The editor clock leaves a game's pause alone".

#### Scenario: Pausing the game clock from TS
- **WHEN** a TS system takes `time: ResMut<Time<Virtual>>` and calls `time.pause()`
- **THEN** `time.is_paused` is `true` in the same run, and from the next frame `Res<Time>`'s `delta_secs` is 0 in `Update`

#### Scenario: Slow motion
- **WHEN** a TS system calls `time.setRelativeSpeed(0.5)` on `ResMut<Time<Virtual>>` with frames of 0.1 s
- **THEN** from the next frame `Res<Time>`'s `delta_secs` is 0.05, as for the same call from Rust

#### Scenario: A faster fixed step
- **WHEN** a TS system calls `time.setTimestepHz(128)` on `ResMut<Time<Fixed>>`
- **THEN** `time.timestep` is 1/128 in the same run, and `FixedUpdate` runs twice as often as at 64 Hz from the next frame

#### Scenario: A zero timestep
- **WHEN** a TS system calls `time.setTimestep(0)`
- **THEN** the call throws naming `FixedTime` and `setTimestep`, the timestep is unchanged and nothing panics

#### Scenario: A TS pause over an editor step
- **WHEN** the editor has paused, a client steps one frame, and in it a TS system calls `time.pause()`, then the client resumes
- **THEN** `Time<Virtual>` stays paused
