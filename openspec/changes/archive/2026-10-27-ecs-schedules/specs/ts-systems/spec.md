## MODIFIED Requirements

### Requirement: System param kinds
The `bevy-ts` module SHALL export these param types, each mapping to Bevy's own:

- `Query<[data...], filter>`: data is a component class (read), `Mut<C>` (write), `Option<C>` or `Option<Mut<C>>` (the row's value or `undefined`), `Ref<C>` (read, plus `isAdded()`, `isChanged()` and `lastChanged()`), `Has<C>` (a `boolean`, no access to `C`, never filters), `AnyOf<[A, B, ...]>` (each as `Option`, at least one present) and `Entity`. The filter is `With<C>`, `Without<C>`, `Changed<C>`, `Added<C>`, `Spawned` (the entity was spawned since this system last ran), `Or<[F1, F2, ...]>` (any of) or a tuple of them (all of). `Changed<C>` and `Added<C>` match rows whose tick is newer than this system's last run.
- `Single<[data...], filter>`: exactly one match, given as that row; `Option<Single<...>>`: zero or one, the row or `undefined`. `Populated<[data...], filter>`: a `Query` with at least one match. See "Single and Populated skip the system".
- `RemovedComponents<C>`: entities that lost `C` since this system last read it (see "RemovedComponents").
- `SystemChangeTick`: `thisRun` and `lastRun`, this run's tick and the system's last-run tick.
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

#### Scenario: Or of two row filters
- **WHEN** a system has `Query<[Entity], Or<[Changed<Health>, Added<Shield>]>>`, and since its last run one entity's `Health` changed and another got `Shield`
- **THEN** it visits exactly those two entities

#### Scenario: Has
- **WHEN** a system has `Query<[Entity, Has<Stunned>]>` over three entities, one with `Stunned`
- **THEN** it visits all three, and the second element is `true` for that one and `false` for the others

#### Scenario: Spawned
- **WHEN** a system with `Query<[Entity], [With<Enemy>, Spawned]>` runs after another system spawned two `Enemy` entities
- **THEN** it visits those two, and none of them on its next run

#### Scenario: AnyOf
- **WHEN** a system has `Query<[AnyOf<[Health, Shield]>]>` over entities with `Health` only, `Shield` only, both, and neither
- **THEN** it visits the first three, with `undefined` for the missing part, and not the fourth

#### Scenario: Ref
- **WHEN** a system has `Query<[Ref<Health>]>` and another system wrote `Health` on one row since its last run
- **THEN** that row's `isChanged()` is `true`, every other row's is `false`, and writing through the `Ref` view throws

#### Scenario: SystemChangeTick
- **WHEN** a system takes `ticks: SystemChangeTick` and runs twice
- **THEN** the second run's `ticks.lastRun` equals the first run's `ticks.thisRun`

## REMOVED Requirements

### Requirement: States
**Reason**: States become their own capability, `ts-states`, which keeps this requirement as "State machines" and adds Bevy states, transitions and state-scoped entities.
**Migration**: None for games: `States`, `initState`, `NextState`, `OnEnter`, `OnExit` and `inState` keep their behaviour.
