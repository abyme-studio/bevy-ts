## ADDED Requirements

### Requirement: Delayed commands
Every TS `Commands` (a system's, an observer's, a one-shot system's, and a delayed one) SHALL offer `delayed()`, Bevy 0.20's `DelayedCommandsExt::delayed`, returning a `DelayedCommands` with `secs(s)` and `duration(s)`. Each SHALL return a `Commands` whose commands run `s` seconds later; `secs` SHALL take its delay as Bevy's `Duration::from_secs_f32` of `s` rounded to `f32`, `duration` as `Duration::from_secs_f64(s)`. Every method of `Commands` and `EntityCommands` SHALL work on a delayed `Commands`, in call order within its queue, with the same semantics as when not delayed. A delay that is negative, `NaN`, infinite or past `Duration`'s range SHALL throw at the call, naming the method and the value, and queue nothing.

Calls on one `DelayedCommands` whose delays make the same `Duration` SHALL share one queue, as Bevy's map keyed by `Duration` does. Each queue SHALL become, when the run's commands apply, one Bevy `DelayedCommandQueue` entity with `submit_at` the default clock's (`Time`) elapsed time plus the delay, so a queue made in `FixedUpdate` counts from `Time<Fixed>`'s elapsed time, as in Bevy. Bevy's `check_delayed_command_queues` SHALL fire it in `PreUpdate` once `Time`'s elapsed time reaches `submit_at`: its commands apply then, as one TS run's commands do (all or nothing, warnings to the script log). So a delay SHALL follow the game clock: it waits while `Time<Virtual>` is paused, by the game or the editor, and runs at the clock's relative speed. Queues that come due on the same frame SHALL keep no order between them, as in Bevy.

`spawn`, `spawnBatch`, `cloneAndSpawn` and `withChildren`'s `spawn` on a delayed `Commands` SHALL return the entity at once, as Bevy allocates it at once; commands queued later, delayed or not, MAY name it. A run that throws SHALL drop its delayed commands with the rest. A `Commands` or `DelayedCommands` kept past its run SHALL throw on use, naming `delayed`.

A queue SHALL belong to the generation that made it. A restart (a Stop, a restarting reload) SHALL despawn every TS queue with no drop warning. A queue that comes due under a later generation SHALL apply nothing and log one warning naming the generation that made it and how many commands it held. `DelayedCommandQueue` SHALL read in TS as the reflected component it is (`submit_at` in seconds, the queue not visible) and SHALL NOT be inserted from TS.

#### Scenario: Bevy's own test, from TS
- **WHEN** with frames of 0.2 s, a TS `Startup` system calls `commands.delayed().secs(0.1).spawn(Dummy)`, `commands.spawn(Dummy)`, then on one `const d = commands.delayed()` calls `d.secs(0.5).spawn(Dummy)` and three `d.duration(1).spawn(Dummy)`
- **THEN** over ten frames the count of `Dummy` entities is, frame by frame, what Bevy's `delayed_queues_should_run_with_time_plugin_enabled` asserts for the same calls in Rust, and three `DelayedCommandQueue` entities exist after the first frame

#### Scenario: Spawn now, despawn later
- **WHEN** a system calls `const e = d.secs(1).spawn(Dummy)` then `d.secs(2).entity(e).despawn()`
- **THEN** `e` does not exist before 1 s of game time, exists from 1 s, and is gone from 2 s

#### Scenario: Paused game clock
- **WHEN** a system queues `commands.delayed().secs(1).spawn(Dummy)` and the game then pauses `Time<Virtual>` for 3 s of wall time
- **THEN** no `Dummy` exists until 1 s of game time has passed after the unpause

#### Scenario: Nested delays add up
- **WHEN** a system calls `commands.delayed().secs(1).delayed().secs(1).spawn(Dummy)`
- **THEN** the `Dummy` spawns once 2 s of game time have passed, not 1 s

#### Scenario: A bad delay
- **WHEN** a system calls `commands.delayed().secs(-1)`
- **THEN** the call throws naming `secs` and `-1`, and the run's other commands apply if the system catches it

#### Scenario: A Stop drops pending queues
- **WHEN** a system queues `commands.delayed().secs(5).spawn(Dummy)`, the client stops, then plays again, and 10 s pass
- **THEN** that `Dummy` never spawns, no `DelayedCommandQueue` from the first run exists, and no drop warning is logged

#### Scenario: A reload drops pending queues
- **WHEN** a system queues `commands.delayed().secs(5).spawn(Dummy)` and a reload applies before 5 s pass
- **THEN** at 5 s nothing spawns and the script log holds one warning naming the old generation and one command
