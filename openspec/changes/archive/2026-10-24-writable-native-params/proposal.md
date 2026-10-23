## Why

An exported param must be read-only today, so TS cannot pause the game (`ResMut<Time<Virtual>>`),
add an asset (`ResMut<Assets<T>>`), draw gizmos or queue commands from Rust. The rule guards JS's
pointers into Bevy's columns, but only structural changes move those, and `ResMut`,
`Query<&mut T>` and `Commands` make none. The same host path also has a bug: `SystemState::get`
spends a world tick on every call, so a read-only param's `is_changed` means "since the last
call by any TS system", not "since this system last ran" (docs/research/writable-native-params.md).

## What Changes

- An exported param may write: `ResMut`, `Query<&mut T>`, `Single<&mut T>`, `ParamSet`,
  `MessageWriter`, `NonSendMut`, and deferred params (`Commands`, `Gizmos`, any `Deferred<T>`).
  Exported methods may take `&self` or `&mut self`. Not breaking: every param that compiles today
  still does.
- Every exported param, read or write, is fetched with `SystemParam::get_param` on the run's
  `UnsafeWorldCell` at the calling TS system's own `(last_run, tick)`. No call spends a tick. A
  native write is stamped with the run's tick, as a TS `Mut` write is; a read's `is_changed`
  means "since this TS system last ran". This fixes today's tick bug.
- Deferred state applies after the run, after the TS system's own output, in the system's
  parameter order. A failed run drops it, as it drops TS's commands.
- A param whose access is unbounded (`Query<EntityMut>`, `FilteredEntityMut`, `DeferredWorld`)
  is refused at registration, naming the param.
- The host rejects a slice argument that overlaps a column or resource row of the run when the
  slice is `&mut` or the param writes that column.
- The dev guard ("wrote to X, which no query of it declares Mut") ignores columns an exported
  param of the system may write.
- An export may claim the Bevy spelling it stands for:
  `#[export(..., stands_for = "ResMut<Time<Virtual>>")]`. A TS system writes
  `time: ResMut<Time<Virtual>>` and gets the export. One export per spelling. Resolution order:
  the `Time<..>` route (script-time), then claimed spellings, then generic `Res`/`ResMut`
  (rust-resources).
- The handshake's `ExportedParam` gains `standsFor`: the next free protocol minor at landing.
- `docs/native.md` drops the read-only rule.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `native-functions`: exported params may write; one fetch path at the run's ticks; deferred
  apply; refused shapes; slice overlap; dev-guard exclusion; `stands_for` claims; the coverage
  table for writing params.

## Impact

- Code: `crates/bevy-ts-native/src/lib.rs` (`ExportParam` bound), `crates/bevy-ts-native-derive/src/lib.rs`
  (`expand_export`: `&mut self`, `stands_for`, the generated call), `crates/bevy-ts/src/host.rs`
  (`HostFn` takes an `UnsafeWorldCell` and ticks; registry claims, access, apply),
  `crates/bevy-ts/src/systems.rs` (`run_one`: cell, ticks, apply, overlap, dev guard),
  `crates/bevy-ts/src/runtime.rs` (`call_entry`), `crates/bevy-ts-cli/src/reader.rs` (claimed
  spellings), `bevy-ts types` (`ResMut<T>` conditional), `protocol/bevy-ts.openrpc.json` and
  regenerated types.
- Ask-first (operator's go needed):
  - Architecture: the host path moves from `&World` to an `UnsafeWorldCell` built from
    `run_one`'s `&mut World`, and every exported param shares one `SystemState` across TS
    systems with ticks supplied per call.
  - Unsafe world access: new `unsafe` blocks for `get_param` on the cell and for the
    slice-overlap guarantee.
  - Performance: research measured a writing call at about 114 ns end to end against today's
    113 ns read; one queued command adds about 18 ns. The build confirms it with a writing arm
    in `native_params_bench.rs`, 3 runs per arm.
- Neighbours:
  - reflect-coverage stage 3 (reflect-enums-color) needs the same writable host call with the
    run's tick. This change owns the mutable cell and run ticks on the host path; stage 3 builds
    on it.
  - bevy-ui-coverage edits the `NativeLayout` derive in the same file as `expand_export`: a
    rebase, not a conflict.
  - script-time owns `VirtualTime`/`FixedTime` and the `Time<C>` declaration; rust-resources owns
    generic `Res`/`ResMut` and checks claims first. Agreed 2026-10-23.
