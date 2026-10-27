## ADDED Requirements

### Requirement: A stall is logged as an error too
Besides its line on stderr, the watchdog SHALL log the same line through Bevy's log at error level, prefixed `stall error: `, when the app has a log, so a reader that keeps only errors, or lines matching `rror` (Bevy writes the level as `ERROR`), keeps the stall. `BEVY_TS_SCREENSHOT`'s shot that gives up SHALL log its error prefixed `shot error: ` for the same reason. The stderr line SHALL stay first and unchanged, starting `Update stopped:`.

#### Scenario: A headless run's log filtered to errors
- **WHEN** a headless run with Bevy's log stalls in its systems, and its stderr is filtered to lines matching `rror`
- **THEN** the filtered output has the line, with `Update stopped:` and the frame

### Requirement: A stall names what a waiting shot waits on
While `BEVY_TS_SCREENSHOT`'s shot, the fixed step or a `bevy-ts/screenshot` waits, the watchdog's line SHALL end with what it waits on, as last counted: whether the script started, `held by the script: ` and the held names, pipelines still waiting, assets still loading, and the ready frames counted. The line SHALL still start `Update stopped:`.

#### Scenario: A stall while the shot waits on a hold
- **WHEN** a run under `BEVY_TS_SCREENSHOT` holds `"abymed"` and a system stops returning
- **THEN** the line starts `Update stopped:` and ends with `the env shot waits on: the script started; held by the script: abymed; ready frame 0 of 60`
