## 1. Host path at the run's ticks

- [x] 1.1 `crates/bevy-ts/src/host.rs`: `HostFn` takes `UnsafeWorldCell<'_>` and the run's `(last_run, tick)`; `WorldHost` holds the cell and ticks; `ParamState` keeps the `SystemState<P>` and a cloned `SystemMeta`
- [x] 1.2 `crates/bevy-ts-native-derive/src/lib.rs` (`expand_export`): accept `&self` and `&mut self`; the generated call sets `meta.set_last_run(last_run)` and calls `P::get_param(state.param_state_mut(), &meta, cell, tick)` with a safety comment
- [x] 1.3 `crates/bevy-ts-native/src/lib.rs`: `ExportParam` bound drops `ReadOnlySystemParam`; remove `ReadOnlyParam`; update the module docs
- [x] 1.4 `crates/bevy-ts/src/systems.rs` (`run_one`) and `crates/bevy-ts/src/runtime.rs` (`call_entry`): build the cell from `&mut World` before JS runs, pass the run's ticks (`call_entry`: its tick and `world.last_change_tick()`)
- [x] 1.5 Tests: `writes_carry_the_ts_systems_tick` and a "calls spend no ticks" test in `crates/bevy-ts-cli/tests/exported.rs`; a read-only param's `is_changed` means "since this system last ran"; Rust `Changed<T>` sees a native write the same frame

## 2. Deferred state and access

- [x] 2.1 `HostParams::register`: call `init_access` and keep the access; refuse write-all components or resources, naming the param
- [x] 2.2 After `rt.apply_output`, call `SystemParam::apply` for each exported param of the system in parameter order; on a throw, rebuild the param's state instead
- [x] 2.3 The run's column and resource-row regions (`host::Region`, built beside the `ParamFrame`s) carry the `ComponentId` they view; the host rejects a slice argument overlapping a frame when the slice is `&mut` or the param writes it, naming param, method and argument
- [x] 2.4 Dev guard in `run_one`: subtract components an exported param of the system may write
- [x] 2.5 Tests: command applied after the run; dropped on throw; two deferred params apply in parameter order; overlap rejected for a writer and for `&mut`, allowed for a reader with `&[T]`; no dev-guard warning on a native write

## 3. `stands_for`

- [x] 3.1 Derive: parse `stands_for = "..."`; `ParamSpec.stands_for`, normalised
- [x] 3.2 Registry: refuse a second claim naming both Rust types; claimed spellings beside `KINDS` (`host::claims()`)
- [x] 3.3 `crates/bevy-ts-cli/src/reader.rs` (`"Res" | "ResMut"` arm): a claimed spelling emits `{ "kind": "port", "name": <ts> }`; unclaimed spellings unchanged; confirm the `Time<..>` route (script-time) and generic `Res`/`ResMut` (rust-resources) order on whichever has landed (neither had on 2026-10-23: the claim check sits first in the arm, before `reference`; script-time's route goes ahead of it, rust-resources' after)
- [x] 3.4 `bevy-ts types`: `ResMut<T>` (and `Res<T>` when claimed) as a conditional over the claims; a claim is typed only when TS declares its inner type (`packages/bevy-ts/test/claims.ts`, checked by `tsc -p test`)
- [x] 3.5 `protocol/bevy-ts.openrpc.json`: `ExportedParam.standsFor`, the next free minor on main at landing; regenerate Rust and TS types; handshake fills it
- [x] 3.6 Tests: a TS system taking `ResMut<Time<Virtual>>` with a test export claiming it pauses virtual time; two claims fail naming both; an unclaimed `ResMut<Time<Fixed>>` behaves as before; handshake carries `standsFor`

## 4. Coverage table, one test per row

- [x] 4.1 Supported, each writes from TS and observes the result: `ResMut<T>`, `Option<ResMut<T>>` absent and present, `ResMut<Assets<T>>`; `Query<&mut T>` with a mixed tuple; `Single<&mut T>` and `Populated<&mut T>` (validation throws); `ParamSet` with writes; `MessageWriter<M>` and `MessageMutator<M>`; `NonSendMut<T>`; `Commands`; `Gizmos` and a custom `Deferred<T>`; `ParallelCommands`; `Local<T>` shared by two TS systems
- [x] 4.2 Refused at registration, message checked: `Query<EntityMut>`, `EntityMutExcept`, `DeferredWorld`; `Query<FilteredEntityMut>` and `FilteredResourcesMut` inert (no access without a builder), tested
- [x] 4.3 Refused at compile time: a `compile_fail` doc test in `crates/bevy-ts-native` exporting a param that is not a `SystemParam` (`&mut World`)

## 5. Bench

- [x] 5.1 `crates/bevy-ts-cli/tests/native_params_bench.rs`: add a writing arm (`ResMut` + `Query<&mut T>`) and a writing + one `Commands` arm beside the read arm
- [x] 5.2 Run it 3 times per arm in release; record ns per call per run and the median in `bench/native-params/results/e2e.txt`; report the numbers to the lead for the operator's go

## 6. Docs and specs

- [x] 6.1 `docs/native.md` "Calling Rust from TS": drop the read-only rule; a param that writes applies in place, deferred params apply after the run; the run's ticks; slice overlap; refused shapes; shared `Local`; `stands_for` with the `Time<Virtual>` example
- [x] 6.2 Module docs in `host.rs` and `bevy-ts-native` say the same
- [x] 6.3 At archive, update `native-functions`' Purpose line, which still says "read-only"

## 7. Gate

- [x] 7.1 Changed crates' tests and clippy while working; workspace build, tests and clippy once, last
- [x] 7.2 Rebase on main; take the next free protocol minor; resolve the `bevy-ts-native-derive` rebase with bevy-ui-coverage if it landed first
