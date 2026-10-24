## ADDED Requirements

### Requirement: TS predicate conditions
This requirement is stage 3 and SHALL ship only after its bench (design) is reported to the operator and approved.

An exported function whose params are read-only system params (`Res<R>`, `Query` without `Mut`, `Local`, `MessageReader`, `State<S>`) and whose return type is `boolean` SHALL be usable as a condition: `.runIf(bossReady)`. `resourceExistsAnd(R, pred)` SHALL gate on `pred` only while `R` exists, `pred` taking `Res<R>` first. A predicate with a writing param (`Mut<C>`, `ResMut`, `Commands`, `MessageWriter`, `NextState`) SHALL fail the load with its location. Evaluating a predicate SHALL be one call into JS. A system gated only by Rust-evaluated conditions SHALL still cost no JS call when skipped.

#### Scenario: A predicate
- **WHEN** `export function bossReady(s: Res<Score>): boolean { return s.value >= 100 }` gates `spawnBoss`
- **THEN** `spawnBoss` runs from the frame `Score.value` reaches 100, and each frame the stage runs costs one JS call for `bossReady`

#### Scenario: A writing predicate
- **WHEN** a predicate takes `ResMut<Score>`
- **THEN** loading fails with "a condition reads only; ResMut<Score> writes", its file and line
