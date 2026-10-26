## ADDED Requirements

### Requirement: TS predicate conditions
This requirement is stage 3 and SHALL ship only after its bench (design) is reported to the operator and approved.

An exported function whose params are read-only system params (`Res<R>`, `Query` without `Mut`, `Local`, `MessageReader`, `State<S>`) and whose return type is `boolean` SHALL be usable as a condition: `.runIf(bossReady)`. `resourceExistsAnd(R, pred)` SHALL gate on `pred` only while `R` exists, `pred` taking `Res<R>` first. A predicate with a writing param (`Mut<C>`, `ResMut`, `Commands`, `MessageWriter`, `NextState`) SHALL fail the load with its location. Evaluating a predicate SHALL be one call into JS. A system gated only by Rust-evaluated conditions SHALL still cost no JS call when skipped.

#### Scenario: A predicate
- **WHEN** `export function bossReady(s: Res<Score>): boolean { return s.value >= 100 }` gates `spawnBoss`
- **THEN** `spawnBoss` runs from the frame `Score.value` reaches 100, and each frame the stage runs costs one JS call for `bossReady`

#### Scenario: A writing predicate
- **WHEN** a predicate takes `ResMut<Score>`
- **THEN** loading fails with "predicate bad: a condition reads only; ResMut<Score> writes", at the predicate's file and line

#### Scenario: A group shares one call
- **WHEN** `systems(a, b, c).runIf(pred)` runs a frame
- **THEN** `pred` is called once; with `distributiveRunIf(pred)` it is called once per member

#### Scenario: A Rust condition first
- **WHEN** `inState(Game.Playing).andThen(bossReady)` gates a system while the game is in `Menu`
- **THEN** `bossReady` is never called

#### Scenario: resourceExistsAnd
- **WHEN** `resourceExistsAnd(Boss, alive)` gates a system and `Boss` is removed
- **THEN** `alive` is no longer called and the system no longer runs

#### Scenario: Rust skips cost no call
- **WHEN** one system is skipped by `inState`, and a predicate's `Single` param finds no entity
- **THEN** neither makes a JS call, and the predicate counts as false

#### Scenario: No boolean
- **WHEN** a predicate returns something other than a boolean
- **THEN** it counts as false and a warning is logged once
