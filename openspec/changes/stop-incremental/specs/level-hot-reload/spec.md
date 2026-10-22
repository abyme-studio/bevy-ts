## MODIFIED Requirements

### Requirement: Identity across reloads
Entities SHALL keep their identity, and so their state, when their name path is unchanged, whatever else moved in the file. A new name path SHALL spawn. A removed name path SHALL despawn with its descendants. Renaming a name by hand SHALL be a despawn and a spawn. An entity despawned by a system SHALL be tombstoned and SHALL NOT respawn on a level reload. `restart: true` SHALL clear tombstones and spawn the level as if first. A Stop SHALL clear tombstones and respawn them, and SHALL keep the entities the play did not change (edit-mode, "Stop restores the levels the play changed").

#### Scenario: A killed enemy stays dead
- **WHEN** a system despawns enemy `a` and the level file is then saved with an unrelated change
- **THEN** enemy `a` does not come back, until a `restart` or a Stop
