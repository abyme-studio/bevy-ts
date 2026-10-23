## MODIFIED Requirements

### Requirement: Schedules, ordering and run conditions
`app.addSystems(schedule, ...systems)` SHALL register systems. `Startup`, `PreUpdate`, `Update`, `PostUpdate` and `FixedUpdate` SHALL map onto the bevy-ts stages; `OnEnter(state)` and `OnExit(state)` onto state transitions (see "States"). Each TS system in a stage SHALL be its own Bevy system in that schedule, in system set `BevyTsSet::<Stage>`, so Rust plugins can order around a stage's TS systems as one set; the `Update` stage SHALL run before `TransformSystems::Propagate`. Each TS system SHALL declare to Bevy exactly the access its params need: a read or a write of each component its queries name, matching the archetypes its filters allow; a read or a write of each resource it names; the access of its exported params. Within a stage, systems SHALL run in registration order, adjusted by ordering: `systems(a, b)` groups systems as Bevy's tuple does, `chain(a, b, c)` orders them, and a group's `.after(x)`, `.before(x)`, `.inSet(set)` and `.runIf(condition)` apply to each member. That run order SHALL be given to Bevy as ordering between the stage's TS systems. `x` is a system function, a set class (`class EnemySet extends SystemSet {}`, named like a component), or a set a Rust crate exposed (`systemSet("rapier:step")`, or a port's typed `PhysicsSet.StepSimulation`). A cycle SHALL fail the load. `inState`, `resourceExists`, `onMessage` and `not` SHALL be Bevy run conditions of the TS system, evaluated in Rust, so a skipped system costs no JS call; a condition's own state SHALL be kept by the system's identity across hot reload. A system's commands, message writes, state changes and triggers SHALL apply at the sync point after the system and before the next TS system of its stage runs. A system in `Startup` SHALL run once per identity per world (hot-reload).

#### Scenario: Two systems in one stage
- **WHEN** `a` and `b` are registered in that order in `Update` and `a` writes a field `b` reads
- **THEN** `b` sees `a`'s write in the same frame

#### Scenario: A spawn is seen by the next system
- **WHEN** `a` spawns an `Enemy` with `commands.spawn` and `b`, registered after `a` in `Update`, queries `Enemy`
- **THEN** `b` sees the new enemy in the same frame

#### Scenario: A skipped system
- **WHEN** `systems(chase).runIf(inState(Game.Playing))` is registered and the state is `Menu`
- **THEN** `chase` is not called and the backend records no call into JS for it

#### Scenario: A TS system is a Bevy system
- **WHEN** `move(q: Query<[Mut<Transform>, Velocity]>)` is registered in `Update`
- **THEN** the `Update` schedule holds one Bevy system named after `move`'s identity, in `BevyTsSet::Update`, whose access writes `Transform` and reads `Velocity`

#### Scenario: A Rust system that conflicts is ordered or reported
- **WHEN** a Rust system in `Update` writes `Transform`, is not ordered against `BevyTsSet::Update`, and the schedule's ambiguity detection is on
- **THEN** Bevy reports the Rust system as ambiguous with `move`, by name

#### Scenario: A reload keeps a condition's state
- **WHEN** `systems(intro).runIf(runOnce())` has run, and the file is saved with a change to `intro`'s body only
- **THEN** `intro` does not run again
