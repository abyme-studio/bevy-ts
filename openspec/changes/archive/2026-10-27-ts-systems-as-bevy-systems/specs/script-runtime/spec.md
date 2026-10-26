## RENAMED Requirements

- FROM: `### Requirement: JS runs on the main thread inside exclusive stage systems`
- TO: `### Requirement: JS runs on the main thread inside TS systems Bevy schedules`

## MODIFIED Requirements

### Requirement: JS runs on the main thread inside TS systems Bevy schedules
The isolate SHALL live on the thread that runs the main app's schedules and SHALL be stored as non-send data. Each TS system SHALL be a Bevy system bound to that thread, holding the runtime, so no two TS systems run at once. A Rust system whose access does not conflict with a TS system's declared access MAY run on another thread while that TS system runs; a Rust system whose access conflicts SHALL NOT. Startup systems, state-transition systems, observers and plugin entry calls SHALL run inside bevy-ts's exclusive systems, as before. Remote protocol handlers that need the runtime SHALL run in `RemoteLast`, also on the main thread.

#### Scenario: Rust systems around a JS stage
- **WHEN** a schedule contains Rust systems ordered before and after the bevy-ts stage set
- **THEN** those Rust systems still run in parallel with each other, and none overlaps a JS system of the set

#### Scenario: A Rust system beside a TS system
- **WHEN** a TS system in `Update` writes `Health`, and an unordered Rust system in `Update` reads only `Fuel`
- **THEN** Bevy may run the Rust system on another thread while the TS system runs, and the TS system's call still runs on the main thread

#### Scenario: A Rust system that writes what TS reads
- **WHEN** an unordered Rust system in `Update` writes `Health` and a TS system in `Update` reads it
- **THEN** the two never run at the same time
