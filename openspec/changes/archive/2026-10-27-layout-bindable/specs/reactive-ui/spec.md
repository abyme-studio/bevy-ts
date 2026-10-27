## ADDED Requirements

### Requirement: A TS system that writes a bound field warns in dev
A bound field SHALL have one writer, its binding. Under `bevy-ts dev`, a TS system run that changes the value of a bound field (a column or a hole) on an entity SHALL log one warning naming the system, `Component.field`, the entity (its `Name`, else its id) and the binding's site, with the site's location. The check SHALL be per field and per entity: a run that writes another field of the same component, or the same field on an entity where it is not bound, SHALL NOT warn. A run that writes the value the field already holds SHALL NOT warn. Each (system, component, field, site) SHALL warn at most once per generation. No warning SHALL be logged at load from a system's declared write set alone. Without `bevy-ts dev` (`dev(false)`, a shipped game) the check SHALL NOT run.

#### Scenario: Writing another field of a bound component
- **WHEN** a scene binds `<Sprite color={() => ...}>` and a TS system with `Query<[Mut<Sprite>]>` sets `sprite.texture_atlas` on that entity every frame
- **THEN** no warning is logged

#### Scenario: Writing the bound field
- **WHEN** the same system sets `sprite.color` on that entity instead
- **THEN** one warning names the system, `Sprite.color`, the entity and the binding's site, and later frames of that generation log no more

#### Scenario: A bound column
- **WHEN** a scene binds `<Transform translation={() => ...}>` and a TS system writes `t.scale` on that entity, then a second system writes `t.translation`
- **THEN** only the second system warns, naming `Transform.translation`

#### Scenario: Not in dev
- **WHEN** the system writing `sprite.color` runs in an app built with `dev(false)`
- **THEN** no warning is logged
