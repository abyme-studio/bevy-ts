## ADDED Requirements

### Requirement: Relationships in the schema and values
A TS component's schema extension SHALL carry an optional `relationship`: its `role` (`source` or `target`), its `counterpart`'s name, and `linkedSpawn` for a target or `allowSelfReferential` for a source. A target's value in a component read SHALL be a JSON array of entity ids. A mutation of a target SHALL be refused with an error naming it. A mutation of a source SHALL re-insert it, so its hooks run. The change SHALL be a minor protocol bump.

#### Scenario: Reading a target over the protocol
- **WHEN** a client reads `game::LikedBy` on `t`, which `a` and `b` like
- **THEN** the value is an array holding `a` and `b`, and the schema names `game::Likes` as its counterpart

#### Scenario: Retargeting over the protocol
- **WHEN** a client mutates `game::Likes.target` on `a` from `b` to `c`
- **THEN** `b`'s `LikedBy` no longer lists `a` and `c`'s does
