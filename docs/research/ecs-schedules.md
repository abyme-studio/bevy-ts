# ECS schedules, conditions, one-shot systems and states from TS (2026-10-23)

Scope: audit row 2 of `bevy-coverage.md` (ECS rows 70-77, 87). Sources are this worktree at
`70880c6` and the Bevy 0.20.0 crates in `~/.cargo/registry` (`bevy_ecs`, `bevy_state`,
`bevy_app`). Confidence is high on what exists today and on Bevy's API (read from source); medium
on the costs, which are estimates from the executor's design and one earlier measurement, not a
bench of this change.

## Recommendation

Land it in three stages. Stage 1 (schedules, Rust-evaluated conditions, one-shot systems) follows
existing patterns. Stage 2 (TS states as real Bevy states) is a new architectural pattern. Stage 3
(a TS predicate condition) is a performance item. Stages 2 and 3 are ask-first for the operator.

## 1. How it works today

States. A TS state is not a Bevy state. `class Game extends States("Menu", "Playing") {}`
(`packages/bevy-ts/index.d.ts:197-204`) becomes a `StateDecl` (`crates/bevy-ts/src/manifest.rs:338-348`)
and a `StateMachine { name, values, current, next }` in the `TsPayloads` resource
(`crates/bevy-ts/src/messages.rs:79-160`), values as `u32` indices remapped by name on reload.
One exclusive system, `run_state_transitions`, sits in Bevy's `StateTransition` schedule
(`crates/bevy-ts/src/lib.rs:152`, `crates/bevy-ts/src/systems.rs:552-582`): it takes each pending
`next`, then runs the TS `OnExit(old)` list and the `OnEnter(new)` list. So no `State<S>`
resource, no `StateTransitionEvent<S>` message, no `PreviousState<S>`, and Bevy's
`DespawnOnExit<S>` cannot name a TS state. The only real Bevy state bevy-ts has is `EngineMode`
(`crates/bevy-ts/src/lib.rs:53`, `:149`).

Schedules. `Stage` is four values, `PreUpdate`, `Update`, `PostUpdate`, `FixedUpdate`
(`crates/bevy-ts/src/manifest.rs:101-117`); each is one exclusive system in `BevyTsSet::<Stage>`
(`crates/bevy-ts/src/lib.rs:154-157`). `ScheduleDecl` adds `Startup`, `OnEnter`, `OnExit` and
`Observer` (`crates/bevy-ts/src/manifest.rs:229-235`). `Startup` is not Bevy's `Startup`: it runs at
generation apply, once per system identity (`crates/bevy-ts/src/generation.rs:474-491`), so it
survives hot reload. Any other schedule is reachable only through a slot around a Rust set
exposed with `ts_set` (`crates/bevy-ts/src/sets.rs:117-178`).

The slot after transform propagation already exists. `bevy:transform-propagate` is an exposed
anchor (`crates/bevy-ts/src/sets.rs:182`, documented in `docs/native.md:108`), so
`addSystems(PostUpdate, systems(s).after(systemSet("bevy:transform-propagate")))` runs after
propagation today. The audit's row 87 is wrong to call it missing. What is missing is a test that
proves it, and that needs a `GlobalTransform` read, which reflect-coverage stage 3 owns.

Conditions. `ConditionDecl` is `InState`, `ResourceExists`, `OnMessage`, `Not`, `All`
(`crates/bevy-ts/src/manifest.rs:237-246`), compiled to `Condition`
(`crates/bevy-ts/src/systems.rs:68-75`, `:244-258`) and evaluated in Rust before the JS call
(`crates/bevy-ts/src/systems.rs:261-271`, `:646-651`). Several `runIf` AND together
(`packages/bevy-ts/runtime/core.js:1105`). A group's `runIf` applies to each member
(`openspec/specs/ts-systems/spec.md:61`), Bevy's `distributive_run_if`, not its collective `run_if`.

One-shot systems: none. Exclusive world access: none, and declined.

## 2. Bevy's names and the TS spelling

TS takes Bevy's name in camelCase where it is a function or method and Bevy's exact name where it
is a type or a label, as `inState`, `OnEnter`, `NextState<S>` do now.

| Bevy 0.20 | Source | TS |
|---|---|---|
| `First`, `PreStartup`, `PostStartup`, `Last` | `bevy_app/src/main_schedule.rs:63-196` | `First`, `PreStartup`, `PostStartup`, `Last` (`Schedule` consts) |
| `FixedFirst`, `FixedPreUpdate`, `FixedPostUpdate`, `FixedLast` | `main_schedule.rs:111-148` | same names |
| custom label, `MainScheduleOrder::insert_after/before` | `main_schedule.rs:214-285` | `class Waves extends ScheduleLabel {}`; `app.insertScheduleAfter(Update, Waves)`; `FixedMainScheduleOrder` likewise |
| `Commands::run_schedule` | `bevy_ecs/src/system/commands/mod.rs:1241` | `commands.runSchedule(Waves)` |
| `and_then`, `or_else`, `nand_then`, `nor_else`, `xor`, `xnor` (and `_eager` forms) | `bevy_ecs/src/schedule/condition.rs:135-592` | methods on `Condition`: `a.andThen(b)`, `a.orElse(b)`, `xor`, ... |
| `run_once` | `condition.rs:648` | `runOnce` |
| `resource_added/changed/removed/equals`, `resource_exists_and_*`, `resource_changed_or_removed` | `condition.rs:728-1095` | `resourceAdded(R)`, `resourceChanged(R)`, `resourceRemoved(R)`, `resourceEquals(new R({...}))`, ... |
| `any_with_component`, `any_component_removed`, `any_match_filter` | `condition.rs:1195-1218` | `anyWithComponent(C)`, `anyComponentRemoved(C)`, `anyMatchFilter(...)` |
| `condition_changed`, `condition_changed_to` | `condition.rs:1299-1355` | `conditionChanged(c)`, `conditionChangedTo(c, true)` |
| `state_exists`, `state_changed` | `bevy_state/src/condition.rs:47`, `:165` | `stateExists(Game)`, `stateChanged(Game)` |
| `register_system`, `run_system`, `run_system_cached` | `bevy_ecs/src/system/system_registry.rs:456`, `:641`, `:814` | `app.registerSystem(fn)`, `commands.runSystem(fn)` |
| `SubStates` (`#[source(Game = Game::Playing)]`), `ComputedStates::compute` | `bevy_state/src/state/sub_states.rs:148`, `computed_states.rs:68` | `class Pause extends SubStates(Game.Playing, "Running", "Paused") {}`; `class InGame extends ComputedStates(Game, (g) => ...) {}` |
| `PreviousState<S>` | `bevy_state/src/state/resources.rs:134` | param `PreviousState<Game>` |
| `OnTransition { exited, entered }` | `bevy_state/src/state/transitions.rs:34` | `OnTransition({ exited: Game.Menu, entered: Game.Playing })` |
| `DespawnOnExit`, `DespawnOnEnter`, `DisableOnExit`, `DisableOnEnter`, `EnableOnExit`, `EnableOnEnter` | `bevy_state/src/state_scoped.rs:145-700` | components of those names taking a state value: `<Entity DespawnOnExit={Game.Playing}>` |

A TS predicate condition is an exported function whose params the compile step reads and that
returns `boolean`: `export function bossReady(s: Res<Score>): boolean`, then `.runIf(bossReady)`.

One-shot systems need no `SystemId` in TS: a TS system already has a stable identity (its module
path), which is what `run_system_cached` keys on. `commands.runSystem(f)` runs `f` when the
caller's commands apply, as Bevy does, nested inside the current stage.

## 3. Cost

- A TS predicate condition is a JS call. The fixed cost of a TS system run is about 3 µs (the call
  into V8 plus view setup, `docs/research/change-detection.md:130`); a predicate with params pays
  about that, per evaluation, every frame its stage runs. Because `runIf` is distributive, a
  predicate on a group of eight systems is eight calls unless it is evaluated once per stage run
  and shared. It also ends the promise that a skipped system costs no JS call
  (`index.d.ts:334`, test `a_skipped_system_costs_no_js_call`). Ask-first.
- Each new schedule TS uses is one more exclusive system, which drains the parallel executor
  (`docs/research/slot-barrier.md`). Make a schedule's stage the first time a generation names
  it, as slots are made (`crates/bevy-ts/src/sets.rs:158-178`), so an unused schedule costs
  nothing. Fixed schedules run once per fixed step, so each of their systems is a JS call per step.
  `PreStartup` and `PostStartup` cost nothing per frame: they are ordering inside
  `run_startups`.
- Rust-evaluated conditions are tick compares, a byte compare (`resourceEquals`) or an archetype
  check (`anyWithComponent`): nanoseconds. Each stateful one (`runOnce`, `resourceChanged`,
  `conditionChanged`, `anyComponentRemoved`) needs its own last-run tick or cursor, as a Bevy
  condition system has; using the gated system's `last_run` (`crates/bevy-ts/src/systems.rs:655`)
  would be wrong, since that only advances when the system runs.
- Real Bevy states cost per TS state machine: one `StateTransitionEvent` message type and the six
  state-scoped-entity systems Bevy adds per type (`bevy_state/src/app.rs:244-275`), each an early
  return on a quiet frame, in `StateTransition` every frame. Estimate 1 to 2 µs per machine per
  frame; not measured. Registered lazily per declared machine.

## 4. Overlap with neighbour teams

| Team | Overlap | Proposed owner |
|---|---|---|
| script-time | `onTimer` conditions | ecs-schedules owns the condition evaluator and adds a seam, `app.ts_condition("time:on-timer", factory)`: a Rust condition system exposed to TS by name, as `ts_set` exposes a set. script-time exposes `onTimer` through it. |
| script-input-devices | `inputJustPressed` conditions | the same seam |
| ecs-queries | `RemovedComponents` cursor; `Or`/`Added` filters | ecs-queries owns the removal cursor (`World::removed_with_id`, `bevy_ecs/src/world/mod.rs:1982`) and filters; `anyComponentRemoved` and `anyMatchFilter` reuse them. Whichever lands first builds the cursor. |
| rust-events | reading `StateTransitionEvent<S>` | free once stage 2 makes TS states real: they are real messages |
| rust-resources | `Res<State<S>>` of a Rust state | rust-resources reads it (`ReflectState`, `bevy_state/src/reflect.rs`); conditions and `OnEnter` on a Rust state are an open question below |
| reflect-enums-color | a Rust state's values in TS | its enum shape (a unit variant as its name) |
| reflect-coverage | inserting `DespawnOnExit<S>` by reflection | for TS states, bevy-ts registers the monomorphs itself in stage 2; nothing to share |

## 5. Making TS states real Bevy states

Bevy states are static types: `State<S>` is one resource per type, `OnEnter(S::X)` a label of
that type, and `SubStates`/`ComputedStates` name their sources as types
(`bevy_state/src/state/state_set.rs:95-140`). TS states are declared at runtime and reload. So
one Rust type per TS machine has to exist at compile time.

Recommended design: a pool of slot types, `TsState<const N: u8>(Sym)`, where `Sym` is a value
name interned for the life of the app (never an index, so reordering or adding values on reload
keeps `DespawnOnExit` components valid). A machine gets a slot the first time a generation
declares it, by path, and keeps it for the app's life, as slots do. bevy-ts keeps driving
transitions itself, so no `FreelyMutableState::register_state` and no Bevy-typed
`SubStates`/`ComputedStates` impls; instead, per slot it registers the
`StateTransitionEvent<TsState<N>>` message, the state-scoped-entity systems, and
`State`/`PreviousState` resources, and splits `run_state_transitions` into three exclusive systems
in Bevy's sets:

- `DependentTransitions`: take every pending `next` (and `NextState<TsState<N>>`, so Rust can set
  a TS state), compute TS sub and computed states in source order, write `State`,
  `PreviousState` and the transition messages.
- `ExitSchedules`: TS `OnExit` lists and Rust `OnExit(TsState::<N>(v))` schedules, deepest
  first.
- `TransitionSchedules` and `EnterSchedules`: `OnTransition`, then `OnEnter`, shallowest first.

A TS computed state's function is a JS call only when a source transitions, not per frame.

Rejected: one Rust type for all machines (one `State` resource holds one value); Bevy-typed
`SubStates` per slot pair (sources are types, so pairs would be combinatorial); and a
bevy-ts-only `DespawnOnExit` handled in `run_state_transitions` (no per-frame cost, but it
re-implements Bevy, and Rust, the inspector and rust-events still see no TS state). The last is
the fallback if the operator declines the slot pool.

## 6. Stages

1. `ecs-schedules` stage 1: every main and fixed schedule (stages made lazily), custom schedules
   with `insertScheduleAfter` and `runSchedule`, `PreStartup`/`PostStartup`, the Rust-evaluated
   conditions and combinators with their own ticks, the `ts_condition` seam, `registerSystem`/
   `runSystem`, and a doc line for the after-propagate slot. Follows existing patterns. Protocol
   minor: the `stage` enum in `protocol/bevy-ts.openrpc.json:2022` gains values.
2. Stage 2: TS states as Bevy states (slot pool), `PreviousState`, `OnTransition`,
   `stateChanged`, `stateExists`, the six state-scoped components, `SubStates`,
   `ComputedStates`. Ask-first: a new architectural pattern and a per-frame cost.
3. Stage 3: the TS predicate condition, with a bench of its per-call cost and the
   once-per-stage-run sharing. Ask-first: performance.

## Open questions for spec

1. Slot pool size: 16 or 32 TS state machines? Each slot is monomorphised code and compile time.
   What happens past the limit: a load error naming the limit?
2. A removed state value on reload: is it an exit (fire `DespawnOnExit`) or does the machine keep
   a stale value until set?
3. Should TS read and gate on Rust states (`inState(EngineMode.Play)`, `OnEnter` of a port's
   state)? That needs a per-type registration, `app.ts_state::<S>()`, and is not in the audit row.
4. Can Rust run a TS one-shot system (`SystemId` by name), and should `runSystem` take an input
   (`runSystemWith`)?
5. Eager combinators (`andEager`, `orEager`): needed, or only the short-circuit forms?
6. Does the predicate condition (stage 3) ship at all, given the "a skipped system costs no JS
   call" promise, or only behind the measurement?
7. `resourceEquals` compares bytes of a layout-tier TS resource. Is a reflect-tier resource in
   scope, or does it wait for rust-resources?
