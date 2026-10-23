# Writable exported params

Exported params can take `ResMut`, `Query<&mut T>` and `Commands` safely: fetch each call with
`SystemParam::get_param` on the run's `UnsafeWorldCell` and the TS system's own ticks, and apply
deferred state after the run. A writing call costs about 114 ns end to end against today's 113 ns; one queued command adds 18 ns.

Researched 2026-10-23 on the shared Apple M5 Max (load average 60 to 70 from other agents),
Bevy 0.20.0, V8 15.0, from `crates/bevy-ts-native-derive/src/lib.rs` (`expand_export`),
`crates/bevy-ts/src/host.rs`, `crates/bevy-ts/src/systems.rs` (`run_one`),
`crates/bevy-ts/src/runtime.rs` (`call_entry`), `crates/bevy-ts-v8/src/lib.rs`
(`op_bevyts_host`) and `bevy_ecs-0.20.0/src/system/function_system.rs` (`SystemState`).
Benches: `bench/native-params/rust` and `crates/bevy-ts-cli/tests/it/native_params_bench.rs`, raw
output in `bench/native-params/results/`.

## 1. Where the read-only rule is enforced, and why

Three places, one reason:

- `crates/bevy-ts-native/src/lib.rs:134`: `ExportParam: ReadOnlyParam`, a blanket alias of Bevy's
  `ReadOnlySystemParam`. A `ResMut` field fails here.
- `expand_export`: every method must take `&self`; `&mut self` is a compile error.
- `host.rs`: `HostFn` takes `&World`, and the generated call uses `SystemState::get(&World)`,
  which needs `ReadOnlySystemParam`. `run_one` hands the host `&*world` while JS runs.

The reason is safety of the borrow, not scheduling and not change detection. The module doc says
it: "read-only, so a method sees the system's writes so far and cannot move or free column
memory". JS holds raw pointers into Bevy's columns, resource rows and tick arrays for the whole
run (`ParamFrame` in `run_one`). The only thing that can invalidate them is a structural change:
a spawn, insert, remove, despawn or resource insert that moves a table. `ResMut`,
`Query<&mut T>` and `Mut<T>` write in place and never move memory; `Commands` defers. So none of
the three asked for breaks the views; only `&mut World`-shaped access does, and that is not a
`SystemParam`.

Scheduling is not a reason. TS systems run inside exclusive slot systems (`run_slot`, "Called
from the slot's exclusive system"), so Bevy runs nothing beside them and never sees the
exported param's access. The audit's claim holds: the call is already exclusive on the main
thread, the borrow ends with the call.

Change detection is not a reason either, but today's path gets it subtly wrong for reads (below).

## 2. Design for writes

Bevy's own shape for a param used outside the executor is `SystemState<P>`: `init_state`,
`init_access`, `get_param(state, meta, world, change_tick)`, then `apply(state, meta, world)` for
what it deferred. Exclusive systems use exactly this (`SystemState::get_mut` + `apply`).
`Gizmos` is `Deferred<GizmoBuffer>` + `Res<GizmoConfigStore>` applied in `apply`;
`ResMut<Time<Virtual>>` is a plain `ResMut`; `Assets<T>` is `ResMut<Assets<T>>` with
`AssetServer` behind `Res` (interior mutability). All four are ordinary `SystemParam`s, so one
mechanism covers them.

### Fetch: the run's world and the run's ticks

`SystemState::get` and `get_mut` both call `world.increment_change_tick()` and set
`meta.last_run` to that new tick, on every call (`function_system.rs:435-458`). Inside a TS run
that is wrong twice:

- A write would be stamped with a fresh tick newer than the TS system's own run tick. Next frame
  the same TS system's `Changed<T>` would see its own native write. Bevy's rule is that a system
  does not see its own writes as changes.
- A read-only param's `Ref::is_changed`, `Changed<T>` or `Res::is_changed` means "since the
  previous call of this param, from any TS system", not "since this TS system last ran", which
  is what TS's own filters mean in the same run. This is today's behaviour for read-only params;
  `todays_get_spends_a_tick_per_call` in the bench crate shows it.

The fix uses public Bevy API only: keep the `SystemState<P>` and a clone of its `SystemMeta`
(`SystemMeta: Clone`); before each call set `meta.set_last_run(last_run)` from the TS system's
memory, then `P::get_param(state.param_state_mut(), &meta, cell, tick)` with the run's `tick`
(`run_one` already has both). `writes_carry_the_ts_systems_tick` proves the result: the row's
changed tick is the run's tick, the next run of the same system does not see it, a system that
last ran earlier does, and no tick is spent. Use this one path for every exported param, read or
write, so the read-only `is_changed` fix comes free. `Mut` sets the column's summary tick itself.

`HostFn` takes an `UnsafeWorldCell<'_>` plus `(last_run, tick)` in place of `&World`.
`run_one` and `call_entry` build the cell from their `&mut World` (they hold it exclusively) and
pass the run's ticks. `ExportParam` drops the `ReadOnlySystemParam` bound; methods may take
`&self` or `&mut self`.

### Borrow rules

- **Structural changes stay out.** Allowed: anything `SystemParam` with no `&mut World`. A
  `Commands` or `Gizmos` param defers to `apply`, after the run, so columns do not move under JS.
- **A write to a column TS holds.** JS is not running during the call, so a native write to a
  column TS reads or writes is sequential, as two Rust statements are: TS's next read sees it.
  A value TS already copied into a JS object is stale, as a copied Rust value is.
- **A slice argument over a column.** `q.tables` hands TS column arrays; TS can pass one as a
  `&[f32]`/`&mut [f32]` argument. If the param also reaches that column and either side writes,
  Rust holds two aliasing borrows, which is undefined behaviour. This already exists for a
  `&mut [T]` slice beside a read-only `Query<&T>`. Rule: the host rejects a slice whose bytes
  overlap a column or resource row of the run when the slice is `&mut` or the param writes that
  component; a few range compares per slice argument, against `ParamFrame`s the run already has.
  The param's write access comes from `init_access` (`SystemState::new` computes it and drops
  it; call `init_access` once more at registration and keep the `SystemAccess`).
- **Within one param**, Bevy's `init_access` already panics on conflicts (two `&mut T`).
- **The dev guard.** `run_one` hashes columns a TS query reads and warns "wrote to X, which no
  query of it declares Mut" when they change. A native write to such a column would trip it
  falsely; subtract the components in the system's exported params' write access.

### Change ticks, the same frame

With the run's ticks, a native write looks like a TS `Mut` write of the same system: same tick,
per row. A later Rust or TS system's `Changed<T>` sees it this frame; Rust's `is_changed` and
TS's agree. `write_marks` (a TS `q.tables` `write(T)`) fills whole tables with the same tick, so
the two cannot disagree.

### Commands apply point

Bevy applies a system's deferred params in parameter order at its next `ApplyDeferred`, or, for
a `SystemState`, when the caller calls `apply`. TS's own `Commands` apply right after the run
(`rt.apply_output` in `run_one`). Exported params apply at the same point, in the TS system's
parameter order, then observers drain as now. A failed run drops its TS commands; it should drop
the param's queue too (Bevy has no precedent; dropping matches TS).

Each exported param has one `SystemState` shared by every TS system that takes it; Bevy gives
each system its own. Sharing is correct here because runs are sequential, the queue is applied
after each run, and ticks come from the calling system. Query state caches archetypes, which is
shared work.

### Worked case: `ResMut<Time<Virtual>>`

For script-time, which owns the param and its module:

```rust
#[derive(SystemParam)]
pub struct VirtualTime<'w> { time: ResMut<'w, Time<Virtual>> }

#[bevy_ts_native::export(ts = "VirtualTime", module = "bevy-ts/time")]
impl VirtualTime<'_> {
    pub fn pause(&mut self) { self.time.pause() }
    pub fn unpause(&mut self) { self.time.unpause() }
    pub fn is_paused(&self) -> bool { self.time.is_paused() }
    pub fn set_relative_speed(&mut self, ratio: f32) { self.time.set_relative_speed(ratio) }
    pub fn relative_speed(&self) -> f32 { self.time.relative_speed() }
}
```

`Time<Virtual>` advances in `First`, so the next frame's delta changes, as in Rust; this frame's
TS `Time` is already built. A negative ratio panics in Bevy and throws in JS as
`VirtualTime.setRelativeSpeed panicked: ...`.

### An export stands for a Bevy type

TS sees only Bevy's names: `export function menu(time: ResMut<Time<Virtual>>) { time.pause() }`,
and `ResMut<Time<Fixed>>` resolves to a `FixedTime` export the same way. The export declares the
spelling it stands for:

```rust
#[bevy_ts_native::export(ts = "VirtualTime", module = "bevy-ts/time", stands_for = "ResMut<Time<Virtual>>")]
```

- **Registry.** `ParamSpec` gains `stands_for: Option<&'static str>`. `HostParams::register`
  refuses a second export claiming the same spelling, naming both Rust types, as it refuses a
  duplicate `ts` name today. The process-wide `KINDS` list, which the reader reads through
  `port_param_kinds()`, gains the claimed spellings beside the names.
- **Reader** (`crates/bevy-ts-cli/src/reader.rs`, the `"Res" | "ResMut"` arm). Today a generic
  argument fails `reference`: "Time<...> is not a resource". Before that check it renders the
  whole param (`ResMut<Time<Virtual>>`, resolved names, no whitespace) and, on a claimed
  spelling, emits `{ "kind": "port", "name": "VirtualTime" }`: the same decl a param typed
  `VirtualTime` gets, so `run_one`, the glue and the call are unchanged.
- **Declarations.** `bevy-ts types` makes `ResMut<T>` resolve to the export's class for each
  claimed spelling, from the registry: `Time` gains a phantom context parameter
  (`Time<C = Virtual>`, with `Real`, `Virtual`, `Fixed` declared) so `Time<Virtual>` and
  `Time<Fixed>` differ structurally (script-time owns that declaration), and `ResMut<T>` becomes a generated conditional,
  `T extends Time<Virtual> ? VirtualTime : T extends Time<Fixed> ? FixedTime : T`. The export's
  own class name stays in `native-modules.d.ts` but a game never has to write it.
- **One owner per spelling: a claim wins.** rust-resources' `Res<T>`/`ResMut<T>` resolve any
  Rust resource with a native layout, `Time<Virtual>` included, so "fail when both" would fire on
  every claim. Instead their resolution checks the export claims first and never resolves a
  claimed spelling; no registration error. script-time's `Time<..>` route sits before both and
  owns every `Res<Time<..>>`: a value built per run (`RunParam::Time`, extended to `Real`,
  `Virtual` and `Fixed`), a plain read with no host call. So `VirtualTime` claims only
  `ResMut<Time<Virtual>>` and `FixedTime` only `ResMut<Time<Fixed>>`; `ResMut<Time>` stays
  refused. `stands_for` is one spelling per export until a case needs more. Agreed with
  rust-resources and script-time 2026-10-23.

## 3. Cost

The JS crossing (glue, V8 fast op, slot codec) is the same op for a read or a write, so the
cost of a write is today's end-to-end call plus the difference in the Rust half.

### Rust half, ns per call (`bench/native-params/rust`, criterion, 3 runs)

1,000 calls per sample over a 10,000-entity world, each call fetching the param and touching one
entity and the resource, as `host.rs` does: downcast the erased state, fetch, call.

| arm | run 1 | run 2 | run 3 | median |
|---|---:|---:|---:|---:|
| `read`: today, `SystemState::get` (`Res` + `Query<&T>`) | 11.3 | 13.3 | 13.1 | 13.1 |
| `write`: proposed, `get_param` at the run's ticks (`ResMut` + `Query<&mut T>`) | 12.4 | 13.7 | 25.2 | 13.7 |
| `write_commands`: `write` + one `Commands` insert per call, applied per run | 29.7 | 31.6 | 31.6 | 31.6 |
| `get_mut`: Bevy's `SystemState::get_mut`, for reference | 15.5 | 23.9 | 13.4 | 15.5 |

Run 3's `write` outlier (25.2, interval 19 to 33) came with load average 60+; its other two runs
sit within 1 ns of `read`. A write fetch costs what a read fetch does; the proposed path is
slightly cheaper than `get_mut` because it spends no tick. A queued command adds about 18 ns
including its share of `apply`, which is Bevy's own command cost.

### End to end, read-only, through V8 (`native_params_bench.rs`, 3 runs)

A TS system making 10,000 calls of a read-only method per run, against the same loop with no
call; median frame of 200, the difference over 10,000.

| run | frame without calls, us | with 10,000 calls, us | ns per call |
|---:|---:|---:|---:|
| 1 | 610 | 2,452 | 184 |
| 2 | 284 | 1,414 | 113 |
| 3 | 905 | 1,907 | 100 |

Median 113 ns. Run 1 ran while the release build of other targets was still finishing. Of a
call's ~113 ns, the Rust half is ~13; the rest is the JS glue and the V8 fast op, which a write
shares unchanged.

### Read against write, end to end

| call | ns per call |
|---|---:|
| today, read-only | 113 |
| writing (`ResMut` + `Query<&mut T>`) | ~114 (113 + 0.6 Rust-half difference) |
| writing + one `Commands` insert | ~132 (113 + 18.5) |

The write row is today's measured crossing plus the measured Rust-half difference, not a
separate end-to-end run: the writing path does not exist yet. The build should add a writing
method to `native_params_bench.rs` and confirm it.


## 4. Neighbours

Checked `abyme call agents list` and each neighbour worktree's diff and open change on
2026-10-23.

- **reflect-enums-color (reflect-coverage)**: overlap. Stage 3 writes reflected
  components and resources "at once through `write`" from a host call during the run and "sets
  the change tick". That is the same need: a host call with write access and the run's tick.
  This change should own the mutable cell and run ticks on the host path; stage 3 builds on it.
- **world-to-screen**: adds a read-only `Cameras` param on `#[export]` and
  `extern crate self as bevy_ts_native`. No conflict; it gets the `is_changed` fix free. Its
  spec says "read-only param", which stays true.
- **text-input**: adds read-only `Keys` and `InputFocus` in `crates/bevy-ts-cli/src/input.rs`.
  No conflict. Setting focus from TS (`ResMut<InputFocus>`) would be a consumer.
- **bevy-ui-coverage**: edits `bevy-ts-native-derive/src/lib.rs` (newtype field named
  `value` in the `NativeLayout` derive), not `expand_export`. Same file, different function: a
  rebase, not a conflict.
- **mesh-primitives, script-gltf, material-alpha**: none of them touches the host path,
  the export macro or `run_one`'s host block.
- **rust-resources** (not on the list, asked us): agreed line. A Rust resource with a native
  layout reaches TS as `Res`/`ResMut` (theirs); one with no layout or a method API
  (`Time<Virtual>`, `Assets<T>`) reaches TS through an exported param (ours).
- **script-time**: consumer, the worked case above.

## Recommendation

One path for every exported param: `get_param` on the run's `UnsafeWorldCell` with the TS
system's `(last_run, tick)`, `&mut self` methods allowed, `apply` after the run in parameter
order, a slice-overlap check, and the dev guard told about native writes. Then
`docs/native.md`'s "A param that writes does not compile" becomes "a param that writes is
applied in place; `Commands` and `Gizmos` apply after the run".
