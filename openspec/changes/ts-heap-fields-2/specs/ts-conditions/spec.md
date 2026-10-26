## MODIFIED Requirements

### Requirement: Built-in conditions with Bevy's names
`bevy-ts` SHALL export these run conditions, each Bevy 0.20's of the same name in camelCase, each evaluated in Rust with no call into JS:

- `inState(S.Value)`, `onMessage(M)`, `not(c)` (as now);
- `runOnce()`;
- `resourceExists(R)`, `resourceAdded(R)`, `resourceChanged(R)`, `resourceExistsAndChanged(R)`, `resourceRemoved(R)`, `resourceChangedOrRemoved(R)`;
- `resourceEquals(R.of({...}))` and `resourceExistsAndEquals(R.of({...}))`, comparing the resource's plain fields with the value's bytes in its native layout and its heap fields (`string`, `T[]`) by value;
- `anyWithComponent(C)`, `anyComponentRemoved(C)`, and `anyMatchFilter(...filters)` with `With(C)`, `Without(C)`, `Added(C)`, `Changed(C)` filter values, named as the filter types;
- `conditionChanged(c)` and `conditionChangedTo(c, value)`.

`R` SHALL be a TS resource or a Rust resource with a native layout; a reflect-tier resource SHALL fail the load naming the resource for the `Equals` forms (rust-resources owns reflect-tier resources). `C` SHALL be any component TS can name. A Bevy condition that takes a closure (`resource_exists_and`) SHALL NOT be a built-in.

#### Scenario: resourceChanged
- **WHEN** `systems(redraw).runIf(resourceChanged(Score))` is registered and a Rust system writes `Score` on frame 3 only
- **THEN** `redraw` runs on frame 3 and not on frames 4 and 5, and is never called into JS when skipped

#### Scenario: resourceEquals
- **WHEN** `runIf(resourceEquals(Score.of({ value: 10 })))` gates `win` and `Score.value` goes from 9 to 10
- **THEN** `win` runs from the frame `Score.value` is 10

#### Scenario: resourceEquals on a string
- **WHEN** `runIf(resourceEquals(Title.of({ text: "won" })))` gates `cheer` and a system sets `title.text` to a new string `"won"`
- **THEN** `cheer` runs from that frame, though the two strings are different allocations

#### Scenario: anyComponentRemoved
- **WHEN** `runIf(anyComponentRemoved(Enemy))` gates `recount` and a Rust system removes `Enemy` from one entity
- **THEN** `recount` runs once in the next run of its stage, and not again until another removal
