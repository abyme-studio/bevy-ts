## MODIFIED Requirements

### Requirement: Write notifications
`bevy-ts/level+watch` SHALL stream, for each level:
- `opened`, when a level root opens, from `app.level`, `add_level`, `level_open` or a sidecar, with its `opener`
- `closed`, when a level root closes
- `loaded`
- `failed`, with diagnostics
- `written`, with hash, lines and the operation id
- `external`, when a change not made by the writer is detected
- `rebased`
- `conflict`

The `written` message SHALL serve as viewport's `bsn` write confirmation.

#### Scenario: abyme keeps an open buffer fresh
- **WHEN** `arena.bsn` is open in abyme's editor and the viewport writes it
- **THEN** abyme receives `written` with the new hash and lines, and can reload its buffer

#### Scenario: The Hierarchy follows an open and a close
- **WHEN** a client watches `level+watch`, then another client opens `levels/room.bsn` and closes it
- **THEN** the watcher receives `opened` for `levels/room.bsn` with `opener: "editor"`, `loaded`, then `closed`
