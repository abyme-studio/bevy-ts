## MODIFIED Requirements

### Requirement: System param kinds
The `bevy-ts` module SHALL export these param types, each mapping to Bevy's own:

- `Query<[data...], filter>`: data is a component class (read), `Mut<C>` (write), `Option<C>` or `Option<Mut<C>>` (the row's value or `undefined`) and `Entity`; the filter is `With<C>`, `Without<C>`, `Changed<C>`, `Added<C>` or a tuple of them. `Changed<C>` and `Added<C>` match rows whose tick is newer than this system's last run.
- `Res<R>` and `ResMut<R>`: a resource, read-only or writable.
- `Local<{ field: type, ... }>`: per-system state with a layout (field types `f32`, `u32`, ... as in ts-components), kept in Rust under the system's identity, so it survives reloads (hot-reload).
- `Commands`: the system's command buffer.
- `MessageReader<M>` and `MessageWriter<M>`: a message type's queue (see "Messages").
- `NextState<S>`, `State<S>` and `PreviousState<S>`: a state machine (ts-states).
- `In<E>`: a one-shot system's input, first param only, `E` an `Event` class (ts-schedules).
- Param kinds that native ports register (for example `ReadRapierContext`), declared in their types package and known to the compile step.

A param type that is none of these, an alias of one, or an object type whose properties are these (a param bundle, Bevy's `#[derive(SystemParam)]`) SHALL fail the load with its file and line.

#### Scenario: A param bundle
- **WHEN** a system takes `{ score, time }: Scoring` where `interface Scoring { score: ResMut<Score>; time: Res<Time> }`
- **THEN** it receives an object with a writable `score` and a read-only `time`

#### Scenario: In anywhere but first
- **WHEN** a registered one-shot system takes `In<SpawnAt>` as its second param
- **THEN** loading fails with "In<SpawnAt> must be the first param", its file and line

### Requirement: Schedules, ordering and run conditions
`app.addSystems(schedule, ...systems)` SHALL register systems in a schedule (ts-schedules lists them) or a state transition (ts-states). Each schedule's TS systems SHALL run in one exclusive bevy-ts stage in system set `BevyTsSet::<Stage>`, so Rust plugins can order around it; the `Update` stage SHALL run before `TransformSystems::Propagate`. Within a stage, systems SHALL run in registration order, adjusted by ordering: `systems(a, b)` groups systems as Bevy's tuple does, `chain(a, b, c)` orders them, and a group's `.after(x)`, `.before(x)` and `.inSet(set)` apply to each member. `x` is a system function, a set class (`class EnemySet extends SystemSet {}`, named like a component), or a set a Rust crate exposed (`systemSet("rapier:step")`, or a port's typed `PhysicsSet.StepSimulation`). A cycle SHALL fail the load.

`.runIf(condition)` takes a condition (ts-conditions). On a group it SHALL be Bevy's collective `run_if`: evaluated once per run of the stage, the first time a member is about to run, and its result applied to every member in that run. `.distributiveRunIf(condition)` SHALL give each member its own instance, evaluated before that member, as Bevy's `distributive_run_if`. On a single system the two SHALL be the same. A skipped system gated only by Rust-evaluated conditions SHALL cost no JS call.

#### Scenario: Two systems in one stage
- **WHEN** `a` and `b` are registered in that order in `Update` and `a` writes a field `b` reads
- **THEN** `b` sees `a`'s write in the same frame

#### Scenario: A skipped system
- **WHEN** `systems(chase).runIf(inState(Game.Playing))` is registered and the state is `Menu`
- **THEN** `chase` is not called and the backend records no call into JS for it

#### Scenario: Collective and distributive
- **WHEN** `systems(a, b).runIf(runOnce())` and `systems(c, d).distributiveRunIf(runOnce())` are registered in `Update`
- **THEN** on the first frame `a`, `b`, `c` and `d` all run; on later frames none of them runs

#### Scenario: A group's condition is evaluated once
- **WHEN** `systems(a, b).runIf(resourceExists(Boss))` is registered and `a` removes `Boss` with commands
- **THEN** `b` still runs in that stage run, as in Bevy, and neither runs on the next frame

## REMOVED Requirements

### Requirement: States
**Reason**: States become their own capability, `ts-states`, which keeps this requirement as "State machines" and adds Bevy states, transitions and state-scoped entities.
**Migration**: None for games: `States`, `initState`, `NextState`, `OnEnter`, `OnExit` and `inState` keep their behaviour.
