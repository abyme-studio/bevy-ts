## MODIFIED Requirements

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
