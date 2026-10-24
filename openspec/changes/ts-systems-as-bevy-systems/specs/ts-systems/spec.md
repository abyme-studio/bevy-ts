## MODIFIED Requirements

### Requirement: Schedules, ordering and run conditions
`app.addSystems(schedule, ...systems)` SHALL register systems in a schedule (ts-schedules lists them) or a state transition (ts-states). Each TS system in a schedule SHALL be its own Bevy system in that schedule, in system set `BevyTsSet::<Stage>`, so Rust plugins can order around a schedule's TS systems as one set; the `Update` stage SHALL run before `TransformSystems::Propagate`. Each TS system SHALL declare to Bevy exactly the access its params need: a read or a write of each component its queries name, matching the archetypes its filters allow; a read or a write of each resource it names; the access of its exported params. Within a stage, systems SHALL run in registration order, adjusted by ordering: `systems(a, b)` groups systems as Bevy's tuple does, `chain(a, b, c)` orders them, and a group's `.after(x)`, `.before(x)` and `.inSet(set)` apply to each member. That run order SHALL be given to Bevy as ordering between the stage's TS systems. `x` is a system function, a set class (`class EnemySet extends SystemSet {}`, named like a component), or a set a Rust crate exposed (`systemSet("rapier:step")`, or a port's typed `PhysicsSet.StepSimulation`). A cycle SHALL fail the load. A system's commands, message writes, state changes and triggers SHALL apply at the sync point after the system and before the next TS system of its stage runs.

`.runIf(condition)` takes a condition (ts-conditions). Every condition SHALL be a Bevy run condition with its own state. On a group it SHALL be Bevy's collective `run_if` on the group's system set: evaluated once per run of the schedule, the first time a member is about to run, and its result applied to every member in that run, wherever the members run (a stage or slots). `.distributiveRunIf(condition)` SHALL give each member its own instance, evaluated before that member, as Bevy's `distributive_run_if`. On a single system the two SHALL be the same. A condition's state (a `runOnce` that ran, a change condition's tick, a timer's time) SHALL belong to that condition's instance, and SHALL be kept by the system's or group's identity across hot reload. A skipped system gated only by Rust-evaluated conditions SHALL cost no JS call.

#### Scenario: Two systems in one stage
- **WHEN** `a` and `b` are registered in that order in `Update` and `a` writes a field `b` reads
- **THEN** `b` sees `a`'s write in the same frame

#### Scenario: A spawn is seen by the next system
- **WHEN** `a` spawns an `Enemy` with `commands.spawn` and `b`, registered after `a` in `Update`, queries `Enemy`
- **THEN** `b` sees the new enemy in the same frame

#### Scenario: A skipped system
- **WHEN** `systems(chase).runIf(inState(Game.Playing))` is registered and the state is `Menu`
- **THEN** `chase` is not called and the backend records no call into JS for it

#### Scenario: Collective and distributive
- **WHEN** `systems(a, b).runIf(runOnce())` and `systems(c, d).distributiveRunIf(runOnce())` are registered in `Update`
- **THEN** on the first frame `a`, `b`, `c` and `d` all run; on later frames none of them runs

#### Scenario: A group's condition is evaluated once
- **WHEN** `systems(a, b).runIf(resourceEquals(Score.of({ value: 0 })))` is registered and `a` sets `Score.value` to 1
- **THEN** `b` still runs in that stage run, as in Bevy, and neither runs on the next frame; with `distributiveRunIf` instead, `b` would not run

#### Scenario: A group split across a stage and a slot
- **WHEN** `systems(a, b).runIf(runOnce())` is registered in `FixedUpdate` with `b` also ordered `.after(systemSet("rapier:step"))`
- **THEN** the condition is evaluated once per run of `FixedUpdate`, and `a` and `b` both run on the first run and neither on later runs

#### Scenario: Two configs with the same condition keep separate state
- **WHEN** `systems(a).runIf(runOnce())` and `systems(b).runIf(runOnce())` are registered and `b` is gated off by `inState` on the first frame
- **THEN** `a` runs once on the first frame, and `b` still runs once on the first frame its state allows

#### Scenario: A TS system is a Bevy system
- **WHEN** `move(q: Query<[Mut<Transform>, Velocity]>)` is registered in `Update`
- **THEN** the `Update` schedule holds one Bevy system named after `move`'s identity, in `BevyTsSet::Update`, whose access writes `Transform` and reads `Velocity`

#### Scenario: A Rust system that conflicts is ordered or reported
- **WHEN** a Rust system in `Update` writes `Transform`, is not ordered against `BevyTsSet::Update`, and the schedule's ambiguity detection is on
- **THEN** Bevy reports the Rust system as ambiguous with `move`, by name

#### Scenario: A reload keeps a condition's state
- **WHEN** `systems(intro).runIf(runOnce())` has run, and the file is saved with a change to `intro`'s body only
- **THEN** `intro` does not run again
