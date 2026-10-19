## ADDED Requirements

### Requirement: Rust exposes system sets under stable names
The native crate SHALL be able to expose a Bevy system set to TS with `app.ts_set("physics:step", Schedule, set)`. A name SHALL be `<crate>:<name>` and unique per engine; a duplicate SHALL fail engine startup naming both registrations. The prebuilt engine SHALL expose bevy-ts's own anchors (`bevy:transform-propagate` in `PostUpdate`, and one per bridge stage). Exposed sets SHALL be listed in `bevy-ts/schema` and in the generated `native.d.ts` as a string union, so a misspelled name is a type error.

#### Scenario: Exposing avian's step
- **WHEN** the entry calls `app.ts_set("physics:step", FixedPostUpdate, PhysicsSystems::StepSimulation)`
- **THEN** `"physics:step"` appears in `bevy-ts/schema` and in the `NativeSet` type in `native.d.ts`

### Requirement: Each exposed set has a fixed before slot and after slot
For each exposed set, the engine SHALL add, at app build time, two exclusive bevy-ts stage systems in that set's schedule: one ordered before the set and one after it. TS systems SHALL be placed into slots by their `before`/`after` declarations and run there in declaration order, exactly as in a bridge stage. A slot with no TS systems in the current generation SHALL be skipped by a run condition. A TS reload SHALL move systems between slots without changing any Bevy schedule.

#### Scenario: A TS system after physics
- **WHEN** a TS system declares `{ after: ["physics:step"], read: [Transform], write: [Thrust] }`
- **THEN** each fixed step it runs after avian's step set and sees the positions avian wrote in that step

#### Scenario: A reload moves a system
- **WHEN** a system's `after` changes from `["physics:step"]` to none and the file is saved
- **THEN** the system runs in its bridge stage from the next frame, and no schedule is rebuilt

### Requirement: Slot placement is validated at load
A TS system SHALL name at most one exposed set across `before` and `after`, and its `stage`, if given, SHALL match that set's schedule; otherwise the generation SHALL fail with an error naming the system and the set. `after` and `before` between TS systems SHALL apply only within one slot or stage; naming a TS system in another slot SHALL fail the load.

#### Scenario: Conflicting placement
- **WHEN** a system declares `stage: "update"` and `after: ["physics:step"]`, which lives in `FixedPostUpdate`
- **THEN** the generation fails with an error naming the system, `physics:step` and both schedules

### Requirement: Rust orders against TS through sets only
Rust systems SHALL order against TS work only through `BevyTsSet::<Stage>` and the slot sets (`BevyTsSlot::before("physics:step")`, `BevyTsSlot::after(...)`), never against an individual TS system.

#### Scenario: A Rust system after a slot
- **WHEN** a Rust system is added with `.after(BevyTsSlot::after("physics:step"))`
- **THEN** it runs after every TS system in that slot, whichever generation is loaded
