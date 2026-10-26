## MODIFIED Requirements

### Requirement: TS reloads re-expand levels inside the generation
A TS generation SHALL re-expand every level instance as part of that generation, because prefab code may have changed. A level entry whose code the new generation lacks or breaks, for example because its prefab was removed or a prop was renamed, SHALL become a placeholder, and the generation SHALL commit. The warning SHALL name the level location of each entry. If a level becomes broken under the new generation, for example because a prefab now binds a field the level overrides, the whole generation SHALL fail under bridge's rule. The old code and the old world SHALL then keep running, and the failure SHALL name the level location. A Stop's rebuild re-applies the live module set, which already passed this check with the level's files: a level the Stop keeps (edit-mode, "Stop restores the levels the play changed"), whose files the Stop found unchanged, SHALL NOT be checked or re-expanded again. Every other open level SHALL be checked as for any generation.

#### Scenario: Removing a prefab a level uses
- **WHEN** `enemies::Enemy` is deleted from TSX and saved, while `arena.bsn` still instances it
- **THEN** the generation commits, each enemy is a placeholder with an `unregistered` entry, and the warnings name `arena.bsn`'s `@enemies::Enemy` lines

#### Scenario: A prefab that starts binding an overridden field
- **WHEN** `Lamp` is changed to bind `PointLight.intensity`, while `#Lamp3` in `arena.bsn` overrides `intensity: 20.0`
- **THEN** the generation fails naming that line, and the lamps keep running under the old code

#### Scenario: A Stop does not check a kept level
- **WHEN** `arena.bsn` is open and unchanged since Play, and a client sets `edit`
- **THEN** the rebuild does not parse or prepare `arena.bsn`

#### Scenario: A Stop checks a changed level
- **WHEN** `room.bsn` was saved during play with a new enemy, and a client sets `edit`
- **THEN** the rebuild checks `room.bsn`, and the room remounts as if first with the new enemy
