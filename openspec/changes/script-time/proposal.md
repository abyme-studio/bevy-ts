## Why

TS reaches one thing from `bevy_time`: `Res<Time>`, four numbers. A game cannot read the wall
clock, ask whether the game clock is paused, read the fixed timestep, or keep a cooldown without
hand-rolling it in a `Local`. And a game's own `Time<Virtual>::pause()`, from Rust today, is undone
the next frame by bevy-ts's editor clock. Research mapped the whole of Bevy 0.20's time surface
(`docs/research/script-time.md`, f8ff7c8). This change covers all of it in four stages; stages 1
to 3 are built here.

## What Changes

Stage 1 (built in this change):

- `Res<Time<Real>>`, `Res<Time<Virtual>>`, `Res<Time<Fixed>>` beside `Res<Time>`, each a read-only
  view with Bevy's getter names as snake_case properties, every `Duration` as seconds. `bevy-ts`
  exports `Time<C>` and the markers `Real`, `Virtual`, `Fixed`.
- `elapsed_secs_wrapped` and `wrap_period` on every clock.
- Any `bevy_time::time::Time<..>` path routes to the Time param, not only names ending `::Time`.
- A test that a `FixedUpdate` system reads the fixed timestep as `time.delta_secs`.
- The editor clock stops undoing a game's pause: it unpauses `Time<Virtual>` only when it paused it.
- `Timer`, `Stopwatch` and `TimerMode` as TS field types of components, resources and `Local`s,
  plain data with methods ported from Bevy's. **Protocol**: `FieldType` gains `timer` and
  `stopwatch`; the next free minor at landing, additions only.
- `docs/native.md` stops calling `Time` a column; a new `docs/time.md` owns time in TS.

Stage 3 (built in this change, on ecs-schedules' `ts_condition` seam, on `main` at 3d641f4):

- Bevy's seven time conditions from `bevy_time::common_conditions` as `bevy-ts` exports in
  camelCase: `onTimer`, `onRealTimer`, `onceAfterDelay`, `onceAfterRealDelay`,
  `repeatingAfterDelay`, `repeatingAfterRealDelay` (each taking seconds) and `paused()`.
- bevy-ts exposes each with `ts_condition` under a `bevy:` name (`bevy:on-timer`), as it exposes
  its `bevy:` set anchors; the factory calls Bevy's own function. Each export is a typed wrapper
  over `nativeCondition`. Evaluated in Rust: a skipped system costs no JS call.
- No protocol change.

Stage 2 (built in this change, on writable-native-params' exports and `stands_for`, on `main` at
584b78c):

- `ResMut<Time<Virtual>>` and `ResMut<Time<Fixed>>` write the clocks with the whole of Bevy's
  setter API in camelCase: `pause`, `unpause`, `toggle`, `setRelativeSpeed`, `setRelativeSpeedF64`,
  `setMaxDelta`; `setTimestep`, `setTimestepSeconds`, `setTimestepHz`, `discardOverstep`; and
  `setWrapPeriod` on both. Behind them, `VirtualTime` and `FixedTime` exported in `bevy-ts/time`,
  claiming those spellings with `stands_for`.
- Each also has the clock's getters as properties under Bevy's names, read live, so
  `time.pause(); time.is_paused` is `true`.
- `#[export]` gains `#[getter]`: a no-argument `&self` method that TS reads as a property.
  **Protocol**: `ExportedParam` gains `properties`; the next free minor at landing, additions only.
- Bevy's setter panics (zero timestep, negative speed, ...) throw in JS naming the method, the
  clock unchanged.
- `ResMut<Time>` and `ResMut<Time<Real>>` still fail the load, now naming the two writable clocks.
- The editor-clock rule is unchanged: a TS pause is a game pause and holds.

Later stages (named here, built when their shapes land):

- Stage 4: `commands.delayed().secs(s)`. Waits on writable-native-params' `Commands`.

Out of scope: `TimeUpdateStrategy`, `TimeSender`/`TimeReceiver`, `Time::advance_by`,
`advance_to`, `Time<Fixed>::accumulate_overstep`.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `ts-systems`: the four clocks as resource views; the `Time<..>` route.
- `ts-components`: `Timer`, `Stopwatch`, `TimerMode` field types.
- `remote-protocol`: `bevy-ts/pause` leaves a game's own pause of `Time<Virtual>` alone.
- `ts-conditions`: Bevy's time conditions under their names (stage 3).
- `native-functions`: `#[getter]` properties on exports; the clocks' coverage row (stage 2).

## Impact

- `crates/bevy-ts/src/systems.rs`: `RunParam::Time(Clock)`, the route, one layout per clock.
- `crates/bevy-ts/src/runtime.rs`: `ScriptClock` records its own pause; `advance_clock`,
  `reset_clock`, `frame_info`.
- `crates/bevy-ts/src/layout.rs`, `crates/bevy-ts-cli/src/{reader,types}.rs`,
  `crates/bevy-ts-v8/js/core.js`, `crates/bevy-ts-v8/js/jsx-runtime.js`, `crates/bevy-ts-levels`:
  the two field types wherever `FieldType` is matched.
- `packages/bevy-ts/index.d.ts`: `Time<C>`, `Real`, `Virtual`, `Fixed`, `Timer`, `Stopwatch`,
  `TimerMode`.
- `protocol/bevy-ts.openrpc.json` and generated types: `FieldType` kinds `timer`, `stopwatch`.
- Stage 3: `crates/bevy-ts/src/time_conditions.rs` (new), called from the plugin's `build`;
  `crates/bevy-ts-v8/js/core.js` and its synced copy `packages/bevy-ts/runtime/core.js`;
  `packages/bevy-ts/index.d.ts`; `native.d.ts`'s `NativeCondition` gains the seven `bevy:` names.
- Stage 2: `crates/bevy-ts-cli/src/time.rs` (new, `ScriptTimePlugin`, `VirtualTime`,
  `FixedTime`), added in `cli.rs` and `types.rs`; `crates/bevy-ts/src/systems.rs` (the refusal
  message); `#[getter]` in `crates/bevy-ts-native-derive`, `crates/bevy-ts/src/host.rs`
  (`MethodSpec`), the port glue in `core.js`, `bevy-ts-cli/src/types.rs` (declarations);
  `protocol/bevy-ts.openrpc.json` `ExportedParam.properties` and generated types.
- Docs: `docs/time.md` (new), `docs/native.md:89` and its exported-params section (`#[getter]`), `docs/scripts.md`'s condition table, the time
  rows of the Bevy coverage audit.
- Operator go: stage 2's `#[getter]` is a new pattern on the export API (a property, beside
  methods); it needs a go, routed by the lead. Without it, stage 2 ships getter methods
  (`time.isPaused()`) instead. Everything else: none needed. No AI prompt or tool, no new
  dependency or service, no data access, nothing in production. Performance: the Time param stays a few words built per run, no
  reflection; a Timer is a plain column like `Vec3`. A protocol minor, additions only. Stage 3
  adds no pattern: it is the port wrapper `docs/native.md` already names (`onTimer(2)` over
  `nativeCondition`), with Bevy's own condition systems; no JS call, no protocol change.
