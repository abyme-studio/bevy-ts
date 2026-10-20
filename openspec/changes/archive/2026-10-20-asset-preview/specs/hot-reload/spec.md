## MODIFIED Requirements

### Requirement: A failed reload keeps the running generation
A syntax error, a module that throws while evaluating, an invalid declaration or a failed migration SHALL leave the previous generation running untouched. When the failing generation is the first, the engine SHALL apply an empty generation instead: no systems, no scenes, an empty world. It SHALL keep serving the protocol, thumbnails and previews (operator ruling 2026-10-20: the studio keeps the project's engine running while the project is open). The error SHALL be printed in the terminal with its TS location and published on the protocol. The next successful save SHALL apply normally, as a first load when the running generation is the empty one.

#### Scenario: A syntax error
- **WHEN** the user saves `game.ts` with an unclosed brace
- **THEN** the cube keeps moving as before, the terminal shows the file and line, and `bevy-ts/reload+watch` reports `failed`

#### Scenario: Opening a project that does not load
- **WHEN** the studio starts the engine on a project whose `game.ts` throws while evaluating
- **THEN** the engine runs with an empty world, `bevy-ts/hello` and `asset_preview` answer, `reload+watch` reports `failed` with the location, and fixing the file applies the game as a first load
