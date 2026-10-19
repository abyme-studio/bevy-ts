## ADDED Requirements

### Requirement: Rust exposes system sets under stable names
A crate (the project's own or a port) SHALL be able to expose a Bevy system set to TS with `app.ts_set("rapier:step", Schedule, set)`. A name SHALL be `<crate>:<name>` and unique per engine; a duplicate SHALL fail engine startup naming both registrations. The prebuilt engine SHALL expose bevy-ts's own anchors (`bevy:transform-propagate` in `PostUpdate`, and one per bridge stage). Exposed sets SHALL be listed in `bevy-ts/schema` and in the generated `native.d.ts` as a string union, so a misspelled name is a type error.

#### Scenario: Exposing avian's step
- **WHEN** the Rapier port calls `app.ts_set("rapier:step", FixedUpdate, PhysicsSet::StepSimulation)`
- **THEN** `"rapier:step"` appears in `bevy-ts/schema` and in the `NativeSet` type in `native.d.ts`

### Requirement: Slots exist only for sets a generation names
For an exposed set, the engine SHALL make two exclusive bevy-ts stage systems in that set's schedule, one ordered before the set and one after it, the first time a generation orders a TS system against the set. Making them SHALL be one schedule edit, applied with that generation at the frame boundary. TS systems SHALL be placed into slots by `.before(systemSet(name))`/`.after(systemSet(name))` and run there in registration order, exactly as in a bridge stage. A slot no generation has named SHALL NOT exist, because an exclusive system is a barrier for the parallel executor even when its run condition would skip it. Later reloads SHALL move systems between existing slots without changing any Bevy schedule; an emptied slot SHALL stay and be skipped until `restart: true`.

#### Scenario: A TS system after physics
- **WHEN** a TS system `thrust(q: Query<[Transform, Mut<Thrust>]>)` is registered as `systems(thrust).after(systemSet("rapier:step"))`
- **THEN** each fixed step it runs after Rapier's step set and sees the positions Rapier wrote in that step

#### Scenario: An exposed set nobody uses
- **WHEN** the Rapier port exposes three sets and no TS system names any of them
- **THEN** the engine's schedules contain no bevy-ts slot systems for them

#### Scenario: A reload moves a system
- **WHEN** a system stops naming `"rapier:step"` and the file is saved
- **THEN** the system runs in its bridge stage from the next frame, and no schedule is rebuilt

### Requirement: Slot placement is validated at load
A TS system SHALL name at most one exposed set across `before` and `after`, and the schedule it is registered in SHALL match that set's schedule; otherwise the generation SHALL fail with an error naming the system and the set. `after` and `before` between TS systems SHALL apply only within one slot or stage; naming a TS system in another slot SHALL fail the load.

#### Scenario: Conflicting placement
- **WHEN** a system is registered in `Update` with `.after(systemSet("rapier:step"))`, which lives in `FixedUpdate`
- **THEN** the generation fails with an error naming the system, `rapier:step` and both schedules

### Requirement: Rust orders against TS through sets only
Rust systems SHALL order against TS work only through `BevyTsSet::<Stage>` and the slot sets (`BevyTsSlot::before("rapier:step")`, `BevyTsSlot::after(...)`), never against an individual TS system.

#### Scenario: A Rust system after a slot
- **WHEN** a Rust system is added with `.after(BevyTsSlot::after("rapier:step"))`
- **THEN** it runs after every TS system in that slot once a generation has named the set, whichever generation is loaded
