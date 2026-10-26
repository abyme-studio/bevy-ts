# native-ordering Specification

## Purpose
How TS systems order against Rust: Bevy system sets exposed under stable names, with slots made around a set the first time a generation names it.

## Requirements

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

### Requirement: Slots exist only for sets a generation names
For an exposed set, the engine SHALL make two slot sets in that set's schedule, `BevyTsSlot::before(name)` ordered before the set and `BevyTsSlot::after(name)` ordered after it, the first time a generation orders a TS system against the set. Each TS system placed in a slot SHALL be its own Bevy system in that slot's set, with its declared access, and the slot's systems SHALL run in registration order, exactly as in a bridge stage. TS systems SHALL be placed into slots by `.before(systemSet(name))`/`.after(systemSet(name))`. A slot set holds no system of its own, so a slot no TS system uses SHALL cost the executor nothing. A reload that keeps every system's identity, declaration and place SHALL NOT change any Bevy schedule. A reload that adds, removes or moves a system, or changes its params, SHALL edit only the schedules that system leaves or joins, once, at the frame boundary where the generation applies.

#### Scenario: A TS system after physics
- **WHEN** a TS system `thrust(q: Query<[Transform, Mut<Thrust>]>)` is registered as `systems(thrust).after(systemSet("rapier:step"))`
- **THEN** each fixed step it runs after Rapier's step set and sees the positions Rapier wrote in that step

#### Scenario: An exposed set nobody uses
- **WHEN** the Rapier port exposes three sets and no TS system names any of them
- **THEN** the engine's schedules contain no bevy-ts systems for them

#### Scenario: A reload moves a system
- **WHEN** a system stops naming `"rapier:step"` and the file is saved
- **THEN** the system runs in its bridge stage from the next frame, and only `FixedUpdate`'s schedule is edited

#### Scenario: A reload that changes only a body
- **WHEN** the file of `thrust` is saved with a change inside its body only
- **THEN** no Bevy schedule is edited and `thrust` runs the new body from the next frame

### Requirement: Slot placement is validated at load
A TS system SHALL name at most one exposed set across `before` and `after`, and the schedule it is registered in SHALL match that set's schedule; any schedule with a bevy-ts stage (ts-schedules) MAY hold such a system. Otherwise the generation SHALL fail with an error naming the system and the set. `after` and `before` between TS systems SHALL apply only within one slot or stage; naming a TS system in another slot SHALL fail the load.

#### Scenario: Conflicting placement
- **WHEN** a system is registered in `Update` with `.after(systemSet("rapier:step"))`, which lives in `FixedUpdate`
- **THEN** the generation fails with an error naming the system, `rapier:step` and both schedules

### Requirement: Rust orders against TS through sets only
Rust systems SHALL order against TS work only through `BevyTsSet::<Stage>` and the slot sets (`BevyTsSlot::before("rapier:step")`, `BevyTsSlot::after(...)`), never against an individual TS system.

#### Scenario: A Rust system after a slot
- **WHEN** a Rust system is added with `.after(BevyTsSlot::after("rapier:step"))`
- **THEN** it runs after every TS system in that slot once a generation has named the set, whichever generation is loaded
