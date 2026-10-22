## MODIFIED Requirements

### Requirement: Rust exposes system sets under stable names
A crate (the project's own or a port) SHALL be able to expose a Bevy system set to TS with `app.ts_set("rapier:step", Schedule, set)`. A name SHALL be `<crate>:<name>` and unique per engine; a duplicate SHALL fail engine startup naming both registrations. The prebuilt engine SHALL expose bevy-ts's own anchors: `bevy:transform-propagate` in `PostUpdate`, `bevy:fixed-main-loop` (`RunFixedMainLoopSystems::FixedMainLoop`) in `RunFixedMainLoop`, and one per bridge stage, `bevy-ts:<stage>` with the stage's name in kebab case (`bevy-ts:update`, `bevy-ts:fixed-last`). A bridge stage's anchor SHALL name its set even before the stage is made. Exposed sets SHALL be listed in `bevy-ts/schema` and in the generated `native.d.ts` as a string union, so a misspelled name is a type error.

#### Scenario: Exposing avian's step
- **WHEN** the Rapier port calls `app.ts_set("rapier:step", FixedUpdate, PhysicsSet::StepSimulation)`
- **THEN** `"rapier:step"` appears in `bevy-ts/schema` and in the `NativeSet` type in `native.d.ts`

#### Scenario: After the fixed main loop
- **WHEN** a TS system is registered as `addSystems(RunFixedMainLoop, systems(interpolate).after(systemSet("bevy:fixed-main-loop")))`
- **THEN** each frame it runs after every fixed step of that frame and before `Update`

#### Scenario: After propagation
- **WHEN** a TS system is registered as `addSystems(PostUpdate, systems(follow).after(systemSet("bevy:transform-propagate")))` and reads `GlobalTransform` of an entity whose `Transform` an `Update` system moved that frame
- **THEN** it reads the propagated value of that frame

### Requirement: Slot placement is validated at load
A TS system SHALL name at most one exposed set across `before` and `after`, and the schedule it is registered in SHALL match that set's schedule; any schedule with a bevy-ts stage (ts-schedules) MAY hold such a system. Otherwise the generation SHALL fail with an error naming the system and the set. `after` and `before` between TS systems SHALL apply only within one slot or stage; naming a TS system in another slot SHALL fail the load.

#### Scenario: Conflicting placement
- **WHEN** a system is registered in `Update` with `.after(systemSet("rapier:step"))`, which lives in `FixedUpdate`
- **THEN** the generation fails with an error naming the system, `rapier:step` and both schedules
