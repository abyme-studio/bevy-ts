## MODIFIED Requirements

### Requirement: A saved `.bsn` file applies to the running world
When a level file changes on disk, bevy-ts SHALL:
1. re-parse it
2. expand its prefab instances in one host call
3. reconcile, at one frame boundary, the file's own level and every level that includes it

No frame SHALL show part of the old file and part of the new. A change the writer made itself SHALL be recognised by its content hash and SHALL write nothing further to the world. A reload whose text is not the file's on disk, because it read the file before a later write, SHALL change nothing in the world: the disk's text SHALL be loaded instead, on every asset event of that reload.

#### Scenario: An agent changes a light
- **WHEN** an agent edits `illuminance: 10000.0` to `20000.0` in `arena.bsn` and saves
- **THEN** within one reload the light is brighter and nothing else is written

#### Scenario: The viewport's own write
- **WHEN** the writer saves enemy `b`'s new position and the file watcher reports the change
- **THEN** the reconcile writes nothing, and `Changed<Transform>` does not fire for enemy `b` a second time

#### Scenario: A reload older than the writer's write
- **WHEN** the watcher's reload of `arena.bsn` read the file, then the writer moved enemy `a` and saved, then the reload finishes
- **THEN** no frame shows enemy `a` back where the reload's text puts it, and `tsx_tree+watch` sends nothing for it
