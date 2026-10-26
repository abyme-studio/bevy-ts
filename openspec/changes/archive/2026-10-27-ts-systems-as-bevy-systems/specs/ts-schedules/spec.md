## MODIFIED Requirements

### Requirement: Every main and fixed schedule
`bevy-ts` SHALL export a `Schedule` constant for each of Bevy 0.20's main and fixed schedules that a Bevy app adds systems to: `PreStartup`, `Startup`, `PostStartup`, `First`, `PreUpdate`, `RunFixedMainLoop`, `Update`, `SpawnScene`, `PostUpdate`, `Last`, `FixedFirst`, `FixedPreUpdate`, `FixedUpdate`, `FixedPostUpdate` and `FixedLast`, with Bevy's names. `app.addSystems(schedule, ...)` SHALL run those systems in that Bevy schedule, in system set `BevyTsSet::<Stage>` of that schedule, each TS system as its own Bevy system (ts-systems). `StateTransition`, `Main` and `FixedMain` SHALL NOT be offered: bevy-ts drives state transitions itself, and the other two only run schedules.

The stages of `PreUpdate`, `Update`, `PostUpdate` and `FixedUpdate` SHALL exist from startup, as now. Every other stage SHALL be made the first time a generation registers a system in its schedule, as one schedule edit applied with that generation at the frame boundary, and SHALL stay for the life of the app, skipped while empty. A schedule no generation has named SHALL have no bevy-ts stage. The `First` stage SHALL run after the generation apply and the runtime pump; the `RunFixedMainLoop` stage SHALL run in `RunFixedMainLoopSystems::BeforeFixedMainLoop` unless the system orders against `bevy:fixed-main-loop` (native-ordering). A system in a fixed schedule SHALL run once per fixed step and read `Time<Fixed>` as `Res<Time>`.

#### Scenario: A system in Last
- **WHEN** `app.addSystems(Last, tally)` is registered and the file loads
- **THEN** from that frame `tally` runs once per frame after every `PostUpdate` system, and before the load the `Last` schedule had no bevy-ts system

#### Scenario: Fixed schedules in order
- **WHEN** systems are registered in `FixedFirst`, `FixedPreUpdate`, `FixedUpdate`, `FixedPostUpdate` and `FixedLast`, and a frame runs two fixed steps
- **THEN** each runs twice, and within each step in that order

#### Scenario: An unused schedule costs nothing
- **WHEN** no generation registers a system in `SpawnScene`
- **THEN** the `SpawnScene` schedule contains no bevy-ts system
