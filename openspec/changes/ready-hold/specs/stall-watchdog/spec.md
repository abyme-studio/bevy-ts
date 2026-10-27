## ADDED Requirements

### Requirement: A stall is logged as an error too
Besides its line on stderr, the watchdog SHALL log the same line through Bevy's log at error level, when the app has a log, so a reader that keeps only errors keeps the stall. The stderr line SHALL stay first and unchanged, starting `Update stopped:`.

#### Scenario: A headless run's log filtered to errors
- **WHEN** a headless run with Bevy's log stalls in its systems, and its stderr is filtered to lines matching `rror`
- **THEN** the filtered output has the line, with `Update stopped:` and the frame
