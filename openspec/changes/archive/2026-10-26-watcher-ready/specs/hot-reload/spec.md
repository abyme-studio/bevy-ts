## ADDED Requirements

### Requirement: A reload report means its files are watched
`bevy-ts dev` SHALL watch every file and missing path of a compile before it reports that compile as `applied` or `failed`, so a save made once the report arrives reloads. An event for a file whose text is the one compiled, or for a missing path that is still missing at the same nearest folder, SHALL start no reload: the file watcher may report writes made before the watch started.

#### Scenario: A save right after the report
- **WHEN** a test sees generation 1 applied and saves `game.ts` at once
- **THEN** generation 2 applies with the new text, and the reload log holds `compiling, applied, compiling, applied` and nothing more

#### Scenario: A late report of the project's own writes
- **WHEN** the watcher starts just after the project's files were written, and the platform reports those writes
- **THEN** no second generation starts
