## Context

Exported params reach TS through `#[bevy_ts_native::export]` and `app.ts_param::<P>()`. Three
things force them read-only today: `ExportParam: ReadOnlySystemParam`
(`crates/bevy-ts-native/src/lib.rs:134`), `&self`-only methods in `expand_export`, and
`HostFn` taking `&World` with the generated call using `SystemState::get`. `run_one` and
`call_entry` hand the host `&*world` while JS runs.

The rule exists to keep JS's raw pointers into columns, resource rows and tick arrays valid for
the run (`ParamFrame`). Only structural changes invalidate them. TS systems run inside exclusive
slot systems, so Bevy schedules nothing beside them. Research, measurements and the agreed
boundaries with rust-resources and script-time are in `docs/research/writable-native-params.md`.

## Goals / Non-Goals

**Goals:**

- Writing params (`ResMut`, `Query<&mut T>`, deferred params) behind one fetch path.
- Correct change ticks for every exported param, fixing the tick-per-call bug.
- `stands_for`, so TS writes `ResMut<Time<Virtual>>` and never a port's class name.

**Non-Goals:**

- `VirtualTime` and `FixedTime` themselves (script-time).
- Reflected writes from a host call (reflect-coverage stage 3).
- Structural changes in place, `&mut World`, exclusive params.
- More than one `stands_for` spelling per export.

## Decisions

### One fetch path at the run's ticks

Keep each param's `SystemState<P>` and a clone of its `SystemMeta`. Per call: set
`meta.set_last_run(last_run)` from the TS system, then
`P::get_param(state.param_state_mut(), &meta, cell, tick)`. Public Bevy API only.

- Over `SystemState::get_mut` + `apply`: `get`/`get_mut` call `increment_change_tick` and set
  `last_run` on every call, which stamps writes with a tick newer than the system's own run and
  makes `is_changed` mean "since the last call anywhere". `writes_carry_the_ts_systems_tick` in
  `bench/native-params/rust` proves the chosen path; `todays_get_spends_a_tick_per_call` shows
  the bug.
- Reads use the same path, so the read `is_changed` fix comes free and there is one owner.
- `call_entry` passes its own incremented tick and `world.last_change_tick()` as `last_run`.

### `HostFn` takes the run's cell and ticks

`HostFn = fn(&mut ParamState, UnsafeWorldCell<'_>, RunTicks, &mut SlotReader, &mut SlotWriter, &mut [&mut [u8]])`.
`WorldHost` holds the cell and ticks instead of `&World`. `run_one` and `call_entry` already
hold `&mut World` exclusively; they build the cell with `as_unsafe_world_cell()` before JS runs
and do not touch the world until the run ends. The `unsafe` is the `get_param` call; its safety
comment cites: exclusive access for the run, `init_access` checked at registration, no other
`SystemParam` live during the call (JS is suspended), and the overlap check for slices.

`ExportParam` drops `ReadOnlySystemParam`; it stays `SystemParam + 'static`. `expand_export`
accepts `&mut self`; the generated call is the same for both receivers.

### One `SystemState` per exported param, not per TS system

Shared, as today. Runs are sequential, deferred state is applied after each run, and ticks come
from the caller, so sharing is correct and query state caches archetypes once. Alternative, one
per TS system: matches Bevy's `Local` per system, costs a state per pair and a registry keyed by
system. Rejected; `Local<T>` in an exported param is documented as shared.

### Deferred apply point

After the run: TS's own output (`rt.apply_output`), then `SystemParam::apply` for each exported
param in the system's parameter order, then observers drain as now. TS's output is one buffer,
so interleaving it with params by position is not possible without splitting it; putting it
first matches "TS commands, then Rust's" in every case that matters and is stated in the docs.

On a throw, the run's deferred state is dropped by rebuilding that param's state
(`SystemState::new`). Bevy has no public "discard" for a param's buffers. Rebuilding also resets
the param's `Local`s and query cache; throws are rare and logged, so this is accepted.

### Access kept at registration

`HostParams::register` calls `init_access` once more and keeps the `FilteredAccessSet`
(`SystemState::new` computes it and drops it). It drives three things:

- Refusal: write-all components or write-all resources fails registration naming the param.
  `&mut World` never gets here: it is not a `SystemParam`.
- Slice overlap: for each slice argument, compare its byte range against the run's
  `ParamFrame`s (columns, resource rows). Reject when the slice is `&mut`, or when the param's
  access writes that component or resource. A few range compares per slice; the frames exist.
  `ParamFrame` gains the `ComponentId` it views so the compare can ask the access.
- Dev guard: `run_one`'s column-hash warning subtracts every component written by an exported
  param of the system.

### `stands_for`

- `ParamSpec.stands_for: Option<&'static str>`, normalised (resolved names, no whitespace).
  `register` refuses a second claim of the same spelling, naming both Rust types, as it refuses
  a duplicate `ts` name. `KINDS` (read by `port_param_kinds()`) gains the claimed spellings.
- Reader (`reader.rs`, `"Res" | "ResMut"` arm): before `reference`, render the whole param; on
  a claimed spelling emit `{ "kind": "port", "name": <ts> }`, the decl a param typed by its class
  gets. `run_one`, glue and the call do not change. The `Time<..>` route (script-time) is
  matched before this arm; generic `Res`/`ResMut` (rust-resources) after it.
- Declarations: `bevy-ts types` emits `ResMut<T>` (and `Res<T>` when claimed) as a conditional
  over the claims, `T extends Time<Virtual> ? VirtualTime : ... : T`. script-time owns
  `Time<C = Virtual>`.
- Protocol: `ExportedParam.standsFor?: string`, the next free minor on main at landing;
  regenerate Rust and TS types.

### Coverage table owner

The table in the spec delta is the category's single list. Rows marked Later name their owner.
reflect-coverage stage 3 consumes this change's writable host call and run ticks and owns
nothing on that path.

## Risks / Trade-offs

- [Unsafe `get_param` on a cell while JS holds raw pointers] → structural changes only through
  deferred params, applied after the run; refusal of unbounded access; overlap check for slices;
  a test per coverage row.
- [A native write leaves a value TS already copied stale] → documented, as for copied Rust
  values; TS's views re-read the column.
- [Shared `Local`] → documented in the table and `docs/native.md`.
- [Rebuild on throw resets `Local`s and query cache] → accepted; throws are logged.
- [Performance] → research: write fetch ~13.7 ns vs read 13.1 ns (Rust half), ~114 ns vs 113 ns
  end to end, +18 ns per queued command. The build adds a writing arm to
  `native_params_bench.rs` and reports 3 runs per arm with the read arm beside it, for the
  operator's go.
- [Neighbour rebase] → bevy-ui-coverage edits the `NativeLayout` derive in the same file.

## Migration Plan

None for users: every param that compiles today still does, with the same TS API. The `is_changed`
of read-only params changes meaning to "since this system last ran", which is the documented
meaning; no exported param in a port, example or test reads `is_changed` today. Rollback is a revert.

## Open Questions

None.
