## ADDED Requirements

### Requirement: Bevy's time conditions under their names
`bevy-ts` SHALL export Bevy 0.20's time run conditions from `bevy_time::common_conditions` in camelCase, each taking a `Duration` as seconds: `onTimer(seconds)`, `onRealTimer(seconds)`, `onceAfterDelay(seconds)`, `onceAfterRealDelay(seconds)`, `repeatingAfterDelay(seconds)` and `repeatingAfterRealDelay(seconds)`; and `paused()`. Each SHALL be Bevy's own condition, exposed by bevy-ts through "Rust exposes conditions under stable names" as `bevy:on-timer`, `bevy:on-real-timer`, `bevy:once-after-delay`, `bevy:once-after-real-delay`, `bevy:repeating-after-delay`, `bevy:repeating-after-real-delay` and `bevy:paused`, and SHALL be evaluated in Rust, so a skipped system costs no JS call. Each SHALL return a `Condition` that combines with the combinators.

The non-real forms SHALL tick by `Res<Time>` of the schedule the system runs in (`Time<Fixed>`'s in `FixedUpdate`), the real forms by `Time<Real>`, and `paused()` SHALL be true while `Time<Virtual>` is paused. A duration that is not a finite number `>= 0` SHALL fail the load with the call's file and line, naming the function. A use's timer SHALL survive a reload while its seconds stay the same; changed seconds, or a Stop, SHALL start it fresh.

#### Scenario: Matching Bevy frame for frame
- **WHEN** a TS system gated by `onTimer(0.25)` and a Rust system gated by Bevy's `on_timer(Duration::from_secs_f64(0.25))` run in `Update` for 20 frames of 0.1 s
- **THEN** both run on the same frames, and the same holds for each of the other five timed conditions

#### Scenario: A timer in the fixed schedule
- **WHEN** a `FixedUpdate` system is gated by `onTimer(0.5)` with the timestep at 1/64 s
- **THEN** it runs on the same fixed steps as a Rust `FixedUpdate` system gated by Bevy's `on_timer` of 0.5 s

#### Scenario: Paused follows the game clock
- **WHEN** a system is gated by `paused()` and a Rust system pauses `Time<Virtual>`, then unpauses it
- **THEN** it runs on every frame while the clock is paused and on none after

#### Scenario: A bad duration
- **WHEN** a module gates a system with `onTimer(-1)`
- **THEN** loading fails with an error naming `onTimer` and `-1`, at that call's line

#### Scenario: A reload keeps the timer
- **WHEN** a system gated by `onTimer(1)` has run 0.6 s of its timer and its module reloads with an unrelated edit
- **THEN** it next runs 0.4 s later, not 1 s later
