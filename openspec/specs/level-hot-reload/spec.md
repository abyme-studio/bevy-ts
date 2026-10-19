# level-hot-reload Specification

## Purpose
What a change to a level, to the TS code under it, or a failed load does to the running world: the `tsx` state rules applied per level, so overrides are controlled, simulation state survives, a bad file is rejected whole and despawned entities stay dead.

## Requirements

### Requirement: A saved `.bsn` file applies to the running world
When a level file changes on disk, bevy-ts SHALL:
1. re-parse it
2. expand its prefab instances in one host call
3. reconcile, at one frame boundary, the file's own level and every level that includes it

No frame SHALL show part of the old file and part of the new. A change the writer made itself SHALL be recognised by its content hash and SHALL write nothing further to the world.

#### Scenario: An agent changes a light
- **WHEN** an agent edits `illuminance: 10000.0` to `20000.0` in `arena.bsn` and saves
- **THEN** within one reload the light is brighter and nothing else is written

#### Scenario: The viewport's own write
- **WHEN** the writer saves enemy `b`'s new position and the file watcher reports the change
- **THEN** the reconcile writes nothing, and `Changed<Transform>` does not fire for enemy `b` a second time

### Requirement: Overrides follow tsx's controlled-field rules
An override value SHALL be written when it differs from the last value applied for that field. It SHALL NOT be written when it is unchanged, even if a system changed the field since. Removing an override SHALL write the value the field would otherwise have: the inner file's override, or the prefab's declaration. A changed prefab prop SHALL re-expand that instance, reconciled with tsx's rules, so `initial*` fields keep their running values.

#### Scenario: A moved enemy survives an unrelated save
- **WHEN** a `bob` system has moved enemy `b`, and the light's line is edited and saved
- **THEN** enemy `b` stays where `bob` put it

#### Scenario: Raising hp in the file
- **WHEN** enemy `b` has taken damage, and `@hp: 250.0` becomes `@hp: 400.0`
- **THEN** `Health.max` becomes 400 and `Health.hp` keeps its damaged value

### Requirement: Identity across reloads
Entities SHALL keep their identity, and so their state, when their name path is unchanged, whatever else moved in the file. A new name path SHALL spawn. A removed name path SHALL despawn with its descendants. Renaming a name by hand SHALL be a despawn and a spawn. An entity despawned by a system SHALL be tombstoned and SHALL NOT respawn on a level reload. `restart: true` SHALL clear tombstones and spawn the level as if first.

#### Scenario: A killed enemy stays dead
- **WHEN** a system despawns enemy `a` and the level file is then saved with an unrelated change
- **THEN** enemy `a` does not come back, until a `restart`

### Requirement: A bad file changes nothing
A level file that fails to parse or resolve SHALL be rejected whole. Failures include an unknown prefab, component, field or name, a type mismatch, a duplicate name, an override of a bound field or an inclusion cycle. Its level, and every level including it, SHALL stay exactly as it was. Each error SHALL carry file, line and column, and SHALL stream on `bevy-ts/level+watch` and the terminal. The next good save SHALL apply against the last good one.

#### Scenario: A typo in a component name
- **WHEN** `Transfrom { … }` is saved at `arena.bsn:12:5`
- **THEN** the terminal shows that location with "unknown component Transfrom (did you mean Transform?)", and the world is unchanged

### Requirement: TS reloads re-expand levels inside the generation
A TS generation SHALL re-expand every level instance as part of that generation, because prefab code may have changed. If any level fails under the new generation, for example because its prefab was removed or a prop was renamed, the whole generation SHALL fail under bridge's rule. The old code and the old world SHALL keep running, and the failure SHALL name the level location.

#### Scenario: Removing a prefab a level uses
- **WHEN** `enemies::Enemy` is deleted from TSX and saved, while `arena.bsn` still instances it
- **THEN** the generation fails naming `arena.bsn`'s `@enemies::Enemy` lines, and the enemies keep running under the old code

### Requirement: Edit mode, selection and the camera survive level reloads
A level reload, whether caused by the writer or by an external change, SHALL keep the viewport's mode, its selection by node id, and the editor camera. The selection SHALL be cleared only when the selected entity is despawned.

#### Scenario: Selection across a write
- **WHEN** enemy `b` is selected, dragged and written
- **THEN** after the reload enemy `b` is still selected with the gizmo on it
