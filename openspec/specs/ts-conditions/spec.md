# ts-conditions Specification

## Purpose
Run conditions for TS systems with Bevy's names and combinators, evaluated in Rust, and conditions Rust crates expose to TS.

## Requirements

### Requirement: Built-in conditions with Bevy's names
`bevy-ts` SHALL export these run conditions, each Bevy 0.20's of the same name in camelCase, each evaluated in Rust with no call into JS:

- `inState(S.Value)`, `onMessage(M)`, `not(c)` (as now);
- `runOnce()`;
- `resourceExists(R)`, `resourceAdded(R)`, `resourceChanged(R)`, `resourceExistsAndChanged(R)`, `resourceRemoved(R)`, `resourceChangedOrRemoved(R)`;
- `resourceEquals(R.of({...}))` and `resourceExistsAndEquals(R.of({...}))`, comparing the resource's bytes with the value's bytes in its native layout;
- `anyWithComponent(C)`, `anyComponentRemoved(C)`, and `anyMatchFilter(...filters)` with `With(C)`, `Without(C)`, `Added(C)`, `Changed(C)` filter values, named as the filter types;
- `conditionChanged(c)` and `conditionChangedTo(c, value)`.

`R` SHALL be a TS resource or a Rust resource with a native layout; a reflect-tier resource SHALL fail the load naming the resource for the `Equals` forms (rust-resources owns reflect-tier resources). `C` SHALL be any component TS can name. A Bevy condition that takes a closure (`resource_exists_and`) SHALL NOT be a built-in.

#### Scenario: resourceChanged
- **WHEN** `systems(redraw).runIf(resourceChanged(Score))` is registered and a Rust system writes `Score` on frame 3 only
- **THEN** `redraw` runs on frame 3 and not on frames 4 and 5, and is never called into JS when skipped

#### Scenario: resourceEquals
- **WHEN** `runIf(resourceEquals(Score.of({ value: 10 })))` gates `win` and `Score.value` goes from 9 to 10
- **THEN** `win` runs from the frame `Score.value` is 10

#### Scenario: anyComponentRemoved
- **WHEN** `runIf(anyComponentRemoved(Enemy))` gates `recount` and a Rust system removes `Enemy` from one entity
- **THEN** `recount` runs once in the next run of its stage, and not again until another removal

### Requirement: Combinators
A `Condition` SHALL offer Bevy's combinators as methods: `a.andThen(b)`, `a.orElse(b)`, `a.nandThen(b)`, `a.norElse(b)`, `a.xor(b)`, `a.xnor(b)`, and the eager forms `a.andEager(b)`, `a.orEager(b)`, `a.nandEager(b)`, `a.norEager(b)`. The short-circuit forms SHALL NOT evaluate `b` when `a` decides the result; the eager forms and `xor`/`xnor` SHALL always evaluate both, so a stateful `b` advances. Several `runIf` calls on one config SHALL AND together, as Bevy's.

#### Scenario: Short-circuit and eager
- **WHEN** `a` is gated by `inState(Game.Playing).andThen(runOnce())`, `b` by `inState(Game.Playing).andEager(runOnce())`, and the game starts in `Menu` and later enters `Playing`
- **THEN** `a` runs once on entering `Playing`; `b` never runs, because its `runOnce` was evaluated, and spent, while in `Menu`

### Requirement: Each condition keeps its own state
Each use of a stateful condition (`runOnce`, the `Added`/`Changed`/`Removed` forms, `anyComponentRemoved`, `conditionChanged`, `conditionChangedTo`) SHALL keep its own last-run tick, cursor or previous value, as a Bevy condition system does, advanced each time the condition is evaluated, not each time the gated system runs. Its state SHALL be kept in Rust per gated config and per position in its condition tree, so a reload that keeps the system's identity and the condition's place keeps it (`runOnce` does not run again), and a Stop or `restart` resets it.

#### Scenario: runOnce across a reload
- **WHEN** a system gated by `runOnce()` has run, and its file is saved with an unrelated edit
- **THEN** it does not run again; after a Stop and play it runs once more

### Requirement: Rust exposes conditions under stable names
A crate SHALL be able to expose a Rust condition to TS with `app.ts_condition("crate:name", factory)`, where `factory` takes the JSON arguments and returns a Bevy `BoxedCondition` or an error, with the naming and uniqueness rules of `ts_set` (native-ordering). TS SHALL use it as `nativeCondition("crate:name", args)`, a `Condition` that combines like any other; ports SHALL export typed wrappers (`onTimer(2)`). The factory SHALL run once per use per generation, at the generation apply; its condition system SHALL be initialized then and run in Rust on each evaluation, keeping its own state as Bevy's does. An error from the factory, or an unknown name, SHALL fail the load naming the condition and the call's location. Exposed conditions SHALL be listed in the generated `native.d.ts` as the `NativeCondition` string union.

#### Scenario: A port's condition
- **WHEN** script-time calls `app.ts_condition("time:on-timer", on_timer_factory)` and a module gates `spawn` with `nativeCondition("time:on-timer", { secs: 2 })`
- **THEN** `spawn` runs every two seconds of virtual time, with no call into JS on the frames between

#### Scenario: Bad arguments
- **WHEN** the factory returns an error for `{ secs: "two" }`
- **THEN** the generation fails with the factory's message, `time:on-timer` and the file and line of the call

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
