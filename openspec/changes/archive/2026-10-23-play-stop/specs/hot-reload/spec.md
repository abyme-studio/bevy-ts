## MODIFIED Requirements

### Requirement: Startup systems run once per identity per world
A system in `Startup` SHALL run once the first time a generation registering its identity is applied, and SHALL NOT run again on later reloads. A `Startup` system with a new identity SHALL run when the generation that adds it is applied. When the game has not started (an engine under `--edit` that has not entered play or stepped, see edit-mode), a `Startup` system SHALL instead run when the game starts, and a generation applied before then SHALL run none. `bevy-ts/reload` with `restart: true` SHALL despawn every entity spawned by TS, forget which `Startup` systems ran, reset `Local` storage, remove every TS resource so the generation inserts it again with its `insertResource` value, clear every TS state and pending TS message, notify registered plugins (see plugin-api), and apply the generation as if first; in edit mode the game is then not started and `Startup` waits for play. A Stop (edit-mode) SHALL reset the world the same way, re-applying the live generation instead of compiling.

#### Scenario: Editing a startup's body
- **WHEN** the body of `Startup` system `scene` changes and reloads
- **THEN** it does not run again and no duplicate cube appears

#### Scenario: A save before the first Play
- **WHEN** an `--edit` engine has not played, `game.ts` gains `Startup` system `spawnWave` and reloads, then a client sets `play`
- **THEN** `spawnWave` runs once, at play, with the other `Startup` systems

#### Scenario: A restart resets the score
- **WHEN** `Score` was inserted with `value: 0`, a system has raised it to 7, and a client calls `bevy-ts/reload` with `restart: true`
- **THEN** `Score.value` is 0 after the restart

#### Scenario: A restart drops a pending message
- **WHEN** a system wrote a `Hit` message this frame and a client calls `bevy-ts/reload` with `restart: true`
- **THEN** no reader sees that `Hit` after the restart
