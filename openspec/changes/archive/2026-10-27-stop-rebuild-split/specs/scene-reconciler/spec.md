## ADDED Requirements

### Requirement: Writing a node back
Given a root with a baseline and its kept resolved tree, `bevy-ts-tsx` SHALL answer, for each changed node, which components changed after the baseline. A changed node SHALL be written back when its archetype equals the recorded one, no entity below it that is not a node changed, and every changed component is either one tsx inserted on it or one Bevy recomputes from the entity's other components before anything reads it (`GlobalTransform`, `TransformTreeChanged`, and the derived list): tsx SHALL insert every component it resolved for the node again with the resolved values, initial fields included, as a spawn would, and SHALL leave the recomputed ones to Bevy. `InheritedVisibility` SHALL NOT be on the list: Bevy recomputes it only when a `Visibility` or the hierarchy changes, not from what a write-back writes. A written-back node's children SHALL be judged on their own: a written-back parent can have a respawned child. The entity, its other components and their change ticks SHALL stay. A node that does not qualify SHALL NOT be written back; its subtree is respawned (`apply_at` with `respawn`). The recomputed list SHALL be one list tsx owns, each entry there because a test shows Bevy recomputes it from components a write-back restores.

#### Scenario: A moved enemy
- **WHEN** a system wrote enemy `b`'s `Transform` and `Health.hp`, both inserted by tsx, and Bevy then updated its `GlobalTransform`
- **THEN** `b` is written back: `Transform` and `Health` hold the level's values, `b` keeps its entity, and its `GlobalTransform` follows on the next frame

#### Scenario: An added component
- **WHEN** a system added `Stunned` to enemy `c`
- **THEN** `c` is not written back

#### Scenario: A component tsx did not insert
- **WHEN** a system changed a component a Rust hook inserted on enemy `d`
- **THEN** `d` is not written back

#### Scenario: An inherited visibility changed
- **WHEN** enemy `b`'s `InheritedVisibility` changed after the baseline
- **THEN** `b` is not written back

#### Scenario: A written-back parent
- **WHEN** a system moved enemy `b` and added `Stunned` to its child `eye`
- **THEN** `b` is written back and keeps its entity, and `eye` is respawned under it
