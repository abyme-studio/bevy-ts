## ADDED Requirements

### Requirement: Scripts have performance.now
Scripts SHALL have the web's `performance.now()`: milliseconds, as an `f64` with sub-millisecond precision, since the engine's script runtime started, read from `bevy_platform`'s monotonic `Instant`. It SHALL never go backwards. `performance.timeOrigin` SHALL be the Unix time in milliseconds when that clock started. No other member of the web's `Performance` interface is promised. A reload SHALL NOT restart the clock.

#### Scenario: Timing a system
- **WHEN** a TS system reads `performance.now()`, busy-loops until `Date.now()` has advanced 5 ms, and reads it again
- **THEN** the difference is at least 5 and is not a whole number on most runs

#### Scenario: Across a reload
- **WHEN** a system logs `performance.now()`, the file is saved, and the new generation logs it again
- **THEN** the second value is greater than the first
