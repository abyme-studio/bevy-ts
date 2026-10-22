## ADDED Requirements

### Requirement: dev --edit remembers the editor's levels
Under `bevy-ts dev --edit`, the engine SHALL keep `<project>/.bevy-ts/editor.json` as `{ "version": 1, "open": [...], "closed": [...] }`:
- `open`: the asset path of every open level whose opener is `editor`
- `closed`: the asset path of every level `level_close` closed and nothing has reopened

Levels opened by `ts`, `rust` or `sidecar` SHALL NOT go in `open`. The engine SHALL write the file whenever either set changes, to a temporary file renamed over it, so the file is always whole.

Before the first frame, the engine SHALL read the file: it SHALL take `closed` as the levels `level_close` closed, then open each `open` path with opener `editor`. A restored level SHALL compose once its files load and the first TS generation has been tried, applied or failed, as an `add_level` level does. Restored levels SHALL list with opener `editor` in the first `bevy-ts/level_list`, and a restored `closed` level SHALL stay closed as if `level_close` had closed it in this process.

A missing, malformed or unknown-version file SHALL start with both sets empty and a warning, and SHALL NOT fail the boot. A remembered level whose file is gone SHALL be skipped with a warning on stderr and left out of the next write. One that no longer composes SHALL stay open with its error, as an `app.level` level does.

Plain `bevy-ts dev`, `bevy-ts play` and `bevy-ts build` SHALL neither read nor write the file.

#### Scenario: Reopening the room
- **WHEN** a client opens `levels/room.bsn` in a `dev --edit` engine, and the engine is stopped and started again with `--edit`
- **THEN** `.bevy-ts/editor.json` has `open: ["levels/room.bsn"]`, and the new engine's first `level_list` lists the room with `opener: "editor"`

#### Scenario: A closed game level stays closed
- **WHEN** `game.tsx` calls `app.level("levels/arena.bsn")`, a client closes the arena in a `dev --edit` engine, and the engine starts again with `--edit`
- **THEN** the arena does not open, after the TS generation too, until a client opens it, when it answers `opener: "ts"`

#### Scenario: A remembered level was deleted
- **WHEN** `editor.json` has `open: ["levels/gone.bsn", "levels/room.bsn"]` and `levels/gone.bsn` no longer exists
- **THEN** the engine boots with the room open, prints a warning naming `levels/gone.bsn`, and its next write of `editor.json` holds only the room

#### Scenario: A malformed file
- **WHEN** `editor.json` is not valid JSON, or has `version: 2`
- **THEN** the engine boots in edit mode with no level the editor opened, and prints a warning

#### Scenario: Plain dev ignores it
- **WHEN** `editor.json` has `open: ["levels/room.bsn"]` and the engine starts with `bevy-ts dev` without `--edit`
- **THEN** the room is not open, and a `level_open` or `level_close` leaves `editor.json` unchanged
