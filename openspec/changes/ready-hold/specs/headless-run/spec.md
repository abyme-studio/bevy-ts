## ADDED Requirements

### Requirement: A script holds a run's ready frames
Every engine SHALL have Bevy resource `ReadyHold`, a set of names, empty at start. A TS system SHALL reach it as param `ReadyHold` from built-in module `bevy-ts/ready`, with:
- `hold(name: string)`: adds `name`; holding a held name changes nothing;
- `release(name: string)`: removes `name`; releasing a name not held changes nothing;
- `isHeld(name: string)`: whether `name` is held.

While any name is held, no frame SHALL be ready, whatever its pipelines and loads, and a frame held SHALL start the ready count again. Under `BEVY_TS_SCREENSHOT` and `BEVY_TS_FIXED_STEP=1`, time SHALL NOT step on a held frame: `TimeUpdateStrategy` stays `ManualDuration(Duration::ZERO)`. Once the last name is released in a frame, the next frame SHALL be ready by the usual rules. A hold set by a `Startup` system of the first generation SHALL hold from the first frame on which the script counts as started, so no step comes before it. Without `BEVY_TS_SCREENSHOT`, `BEVY_TS_FIXED_STEP=1` or a waiting `bevy-ts/screenshot`, a hold SHALL change nothing.

A hold SHALL NOT be progress. When the env shot fails after 60 s with no progress while a name is held, its error SHALL say `held by the script: ` and the held names, joined by `, `, beside anything else not ready.

#### Scenario: Data that arrives after the engine starts
- **WHEN** a script holds `"data"` in `Startup`, and releases it in the system that spawns a looping glTF clip once a reply comes, the reply coming after a wall delay of 0 ms in one run and 400 ms with 30 ms of extra work a frame in two more
- **THEN** the three shots are byte-identical

#### Scenario: The same runs without a hold
- **WHEN** the runs above do not hold
- **THEN** the 400 ms run's shot differs from the 0 ms run's

#### Scenario: No time passes while held
- **WHEN** a run under the fixed step holds for 20 frames with nothing loading and no pipeline waiting
- **THEN** `Time<Virtual>` elapsed is 0 on each of them, and 1/60 s on the first ready frame after the release

#### Scenario: A hold never released
- **WHEN** a script holds `"abymed"` and never releases it
- **THEN** after the no-progress limit no file is saved, the error names the path and says `held by the script: abymed`, and the run exits with code 1

#### Scenario: A hold in a window
- **WHEN** `bevy-ts dev` with a window, no shot and no fixed step runs a script that holds for ever
- **THEN** time runs as it does without the hold
