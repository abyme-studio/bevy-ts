## ADDED Requirements

### Requirement: Relationships survive a reload
A component's relationship role (plain, source or target), its counterpart, `linkedSpawn` and `allowSelfReferential` SHALL count as its layout: a change SHALL register a new `ComponentId` with hooks for the new role. When either half of a pair gets a new id, sources SHALL migrate by field name as any layout change does, and every target list SHALL be rebuilt from the sources under the target's current id, with no hook running during the migration. A target SHALL never be migrated by bytes. A pair whose ids are reused SHALL keep its lists untouched.

#### Scenario: A source gains a field
- **WHEN** `Likes` gains `since: f32 = 0` and reloads
- **THEN** every entity keeps its `Likes` target, and every `LikedBy` lists the same entities

#### Scenario: A target becomes linked
- **WHEN** `LikedBy` changes to `RelationshipTarget(Likes, { linkedSpawn: true })` and reloads
- **THEN** `LikedBy` gets a new id, every list holds the same entities, and despawning a target now despawns its sources

#### Scenario: A plain component becomes a source
- **WHEN** `class Likes extends Component { declare target: Entity }` becomes `extends Relationship` with a new `LikedBy` target and reloads
- **THEN** every entity keeps its `target`, and each target entity has a `LikedBy` listing them
