## MODIFIED Requirements

### Requirement: A bad file changes nothing
A level file that is broken SHALL be rejected whole. A broken file is one with a parse error, a duplicate name, an override of a bound field, an inclusion cycle or a missing include. Its level, and every level including it, SHALL stay exactly as it was. Each error SHALL carry file, line and column, and SHALL stream on `bevy-ts/level+watch` and the terminal. The next good save SHALL apply against the last good one.

A file that parses but names code that is missing, failed or does not fit (an unknown prefab, component, prop or field, or a type mismatch) SHALL NOT be rejected under `bevy-ts dev`. It SHALL apply, with placeholders for those entries as `level-placeholders` says, and each SHALL stream as a warning with its location.

#### Scenario: A typo in a component name
- **WHEN** `Transfrom { … }` is saved at `arena.bsn:12:5`
- **THEN** the terminal shows that location with "unknown component Transfrom (did you mean Transform?)" as a warning, the rest of the save applies, and the entity has an `unregistered` entry for `Transfrom`

#### Scenario: An unclosed brace
- **WHEN** `arena.bsn` is saved with an unclosed `{` at line 12
- **THEN** the terminal shows that location, `level+watch` sends `failed`, and the world is unchanged

### Requirement: TS reloads re-expand levels inside the generation
A TS generation SHALL re-expand every level instance as part of that generation, because prefab code may have changed. A level entry whose code the new generation lacks or breaks, for example because its prefab was removed or a prop was renamed, SHALL become a placeholder, and the generation SHALL commit. The warning SHALL name the level location of each entry. If a level becomes broken under the new generation, for example because a prefab now binds a field the level overrides, the whole generation SHALL fail under bridge's rule. The old code and the old world SHALL then keep running, and the failure SHALL name the level location.

#### Scenario: Removing a prefab a level uses
- **WHEN** `enemies::Enemy` is deleted from TSX and saved, while `arena.bsn` still instances it
- **THEN** the generation commits, each enemy is a placeholder with an `unregistered` entry, and the warnings name `arena.bsn`'s `@enemies::Enemy` lines

#### Scenario: A prefab that starts binding an overridden field
- **WHEN** `Lamp` is changed to bind `PointLight.intensity`, while `#Lamp3` in `arena.bsn` overrides `intensity: 20.0`
- **THEN** the generation fails naming that line, and the lamps keep running under the old code
