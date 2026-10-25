## ADDED Requirements

### Requirement: State machines
`class S extends States("Value", ...) {}` SHALL declare a state machine whose values are `S.Value`, named like a component (a Rust-style path from its module, such as `game::Game`, or `static path`). `app.initState(S.Initial)` SHALL create it. `NextState<S>.set(S.Value)` SHALL queue a transition applied in Bevy's `StateTransition` schedule, which SHALL run `OnExit(old)` systems, then `OnEnter(new)` systems. `inState(S.Value)` SHALL be a Rust-evaluated run condition (ts-conditions).

#### Scenario: Entering Playing
- **WHEN** a system calls `next.set(Game.Playing)` from `Menu`
- **THEN** in the next `StateTransition`, `OnExit(Game.Menu)` systems run, then `OnEnter(Game.Playing)` systems, and systems with `runIf(inState(Game.Playing))` run from that frame on

### Requirement: TS states are Bevy states
This requirement and the rest of this capability are stage 2 and SHALL ship only after the states bench (design) is reported to the operator and approved.

Each TS state machine SHALL be backed by a Rust state type `bevy_ts::TsState<N>`, one of a pool of 32, assigned the first time a generation declares the machine, by path, and kept for the life of the app. Its value SHALL be the value's name, interned for the life of the app, so adding or reordering values on reload keeps every stored value valid. For each assigned machine, the world SHALL hold Bevy's `State<TsState<N>>`, `PreviousState<TsState<N>>` once it has transitioned, and `NextState<TsState<N>>`, and SHALL send Bevy's `StateTransitionEvent<TsState<N>>` on each transition, so Rust, the inspector and rust-events see TS states as Bevy states. Rust SHALL find a machine's slot by path (`TsStates::slot("game::Game")`). A Rust write of `NextState<TsState<N>>` SHALL queue a transition as a TS `next.set` does. A 33rd machine SHALL fail the load with "at most 32 state machines", naming the machine.

#### Scenario: Rust reads a TS state
- **WHEN** a TS system sets `Game.Playing` and a Rust system reads `Res<State<TsState<N>>>` for `game::Game`'s slot the next frame
- **THEN** it reads `Playing`, and a Rust `MessageReader<StateTransitionEvent<TsState<N>>>` read one message from `Menu` to `Playing`

#### Scenario: A reorder on reload
- **WHEN** `Game` is `Playing` and its values are reordered in the source and saved
- **THEN** the state is still `Playing` and no transition runs

### Requirement: Transition systems run on one thread
`BevyTsPlugin` SHALL set Bevy's `StateTransition` schedule to Bevy's `SingleThreadedExecutor`, for every state in the app, Rust's included: its many small systems cost far less on one thread (bench 2.1). A game SHALL be able to opt back into the multi-threaded executor with Bevy's own `app.edit_schedule(StateTransition, |s| { s.set_executor(MultiThreadedExecutor::new()); })` after adding the plugin, and TS states SHALL keep working under it.

#### Scenario: Opting back in
- **WHEN** a game sets `StateTransition` to `MultiThreadedExecutor` after `BevyTsPlugin` and a TS system sets `Game.Playing`
- **THEN** `OnExit(Game.Menu)` and `OnEnter(Game.Playing)` systems still run, in that order

### Requirement: A value removed on reload
When a reload removes the current value of a machine, the next `StateTransition` SHALL transition the machine to its `initState` value, as a normal transition: exit systems and exit-scoped components of the removed value SHALL apply. A removed value's `OnEnter`/`OnExit` systems are gone with the old generation and SHALL NOT run.

#### Scenario: Removing Paused
- **WHEN** `Game` is `Paused`, entities carry `DespawnOnExit={Game.Paused}`, and `Paused` is deleted from the source
- **THEN** after the reload the state is the `initState` value and those entities are despawned

### Requirement: Transition order
Transitions SHALL follow Bevy's `StateTransitionSystems`: in `DependentTransitions`, every pending value is taken and sub and computed states are recomputed in source order; then in `ExitSchedules` exit work runs deepest machine first; then in `TransitionSchedules` and `EnterSchedules`, `OnTransition` and enter work run shallowest first. Exit work SHALL be TS `OnExit` systems, Rust `OnExit(TsState::<N>(..))` schedules and exit-scoped components; enter work likewise.

#### Scenario: Leaving a parent
- **WHEN** `Pause` is a sub state of `Game.Playing`, the state is `Playing`/`Paused`, and `Game` is set to `Menu`
- **THEN** `OnExit(Pause.Paused)` runs before `OnExit(Game.Playing)`, then `OnEnter(Game.Menu)` runs

### Requirement: OnTransition and PreviousState
`OnTransition({ exited: S.A, entered: S.B })` SHALL be a schedule whose systems run on exactly that transition, between exit and enter work. `PreviousState<S>` SHALL be a system param giving the value before the last transition, or `undefined` before the first.

#### Scenario: Menu to Playing
- **WHEN** systems are in `OnTransition({ exited: Game.Menu, entered: Game.Playing })` and in `OnTransition({ exited: Game.Paused, entered: Game.Playing })`, and the state goes from `Menu` to `Playing`
- **THEN** only the first runs, and a system reading `PreviousState<Game>` that frame reads `Menu`

### Requirement: State conditions
`stateChanged(S)` SHALL be true in the frame a machine transitioned, and `stateExists(S)` while it has a value, both Rust-evaluated, as Bevy's `state_changed` and `state_exists`.

#### Scenario: Reacting to any change
- **WHEN** `updateHud` is gated by `stateChanged(Game)` and the state changes on frame 5
- **THEN** `updateHud` runs on frame 5 only

### Requirement: State-scoped entities
`DespawnOnExit`, `DespawnOnEnter`, `DisableOnExit`, `DisableOnEnter`, `EnableOnExit` and `EnableOnEnter` SHALL be components taking a state value, written in TSX as a component element (`<DespawnOnExit value={Game.Playing} />`), inserted with commands (`DespawnOnExit.of(Game.Playing)`) and in levels (`DespawnOnExit { value: "game::Game.Playing" }`), and SHALL be Bevy's own components of `TsState<N>`, run by Bevy's systems: despawn, insert `Disabled`, or remove `Disabled` on the named exit or enter.

#### Scenario: Leaving Playing
- **WHEN** an enemy is spawned with `DespawnOnExit.of(Game.Playing)` and the state goes to `Menu`
- **THEN** the enemy is despawned in that `StateTransition`, before `OnEnter(Game.Menu)` systems run

### Requirement: Sub and computed states
`class Pause extends SubStates(Game.Playing, "Running", "Paused") {}` SHALL declare a machine that exists only while its source is in that value, entered at its first value, as Bevy's `SubStates`. `class InGame extends ComputedStates(Game, (g) => g === Game.Playing || g === Game.Paused ? InGame.Yes : undefined, "Yes") {}` SHALL declare a machine whose value is computed from its sources, `undefined` meaning absent, as Bevy's `ComputedStates`; several sources SHALL be passed as an array. The compute function SHALL be a JS call only when a source transitions. A sub state's `NextState` SHALL be ignored while it does not exist. Each counts against the pool. `PreviousState` of a sub or computed state SHALL read `undefined`: Bevy gives no way to keep it for states it does not derive itself.

#### Scenario: A pause sub state
- **WHEN** the state is `Menu` and a system sets `NextState<Pause>` to `Paused`
- **THEN** nothing happens; after `Game` becomes `Playing`, `Pause` exists at `Running`

### Requirement: Rust states in TS
A crate SHALL be able to make a Rust `States` type reachable from TS with `app.ts_state::<S>()`, for a `FreelyMutableState` enum that reflects its values as unit variants. TS SHALL import it from the generated built-in module `bevy-ts/states` (`import { EngineMode } from "bevy-ts/states"`, then `EngineMode.Play`), typed in the generated `native.d.ts`, and use it in `inState`, `stateChanged`, `stateExists`, `OnEnter`, `OnExit`, `OnTransition`, `State<S>`, `NextState<S>`, `PreviousState<S>` and the state-scoped components. bevy-ts SHALL register `EngineMode` this way.

#### Scenario: Gating on EngineMode
- **WHEN** a TS system is gated by `inState(EngineMode.Play)`
- **THEN** it runs only in play, evaluated in Rust
