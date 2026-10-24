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

## REMOVED Requirements

### Requirement: States
**Reason**: States become their own capability, `ts-states`, which keeps this requirement as "State machines" and adds Bevy states, transitions and state-scoped entities.
**Migration**: None for games: `States`, `initState`, `NextState`, `OnEnter`, `OnExit` and `inState` keep their behaviour.
