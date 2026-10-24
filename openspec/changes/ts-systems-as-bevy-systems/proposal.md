## Why

Every TS system in a schedule runs inside one exclusive bevy-ts stage system (`systems.rs`
`run_stage`), so Bevy's executor sees one barrier, not the systems. It cannot order a TS system
against a Rust one, report an ambiguity, trace a TS system or run anything beside it. This is
stage 1 of lanes, the threading architecture the operator approved on 2026-10-23
(`docs/research/script-threading.md`): each TS system becomes a real Bevy system that declares its
access. Everything still runs on the main thread, so behaviour does not change. Stage 2 (more
isolates on threads of their own) then only changes where a call runs.

## What Changes

- Each TS system in a schedule or a slot is its own Bevy system, built at commit from its
  declaration. Its access is Bevy's own: one `QueryState<FilteredEntityMut>` per TS query, built
  with `QueryBuilder` (the builder `QueryParamBuilder` runs) and declared with
  `QueryState::init_access`; resources declared as `Res`/`ResMut` declare them; plus the access of
  its exported params (writable-native-params). Conflicts between TS and Rust systems are Bevy's
  to detect, and ambiguity detection sees TS systems by name.
- A schedule's TS systems run in `BevyTsSet::<Stage>` and stay chained in today's run order
  (registration order adjusted by `after`/`before`/`inSet`). Systems in a slot run in its
  `BevyTsSlot` set. Rust systems that touch none of a TS system's data may run while it runs.
- Change ticks are Bevy's: a TS system's `last_run`/`this_run` come from its `SystemMeta`. A
  rebuilt system keeps its `last_run` across reload.
- Commands, message writes, `NextState` and `commands.trigger` go to the system's deferred buffer,
  applied at the sync point Bevy inserts after it. Because the chain keeps that sync point, the
  next TS system sees them, as today. A system that declares none of `Commands`, `MessageWriter`,
  `NextState` or a deferred exported param has no buffer and no sync point.
- TS run conditions become each system's Bevy `run_if`, a condition system with declared access.
  A skipped system still costs no JS call. Each condition's state (`runOnce`, `resourceChanged`'s
  tick, `onTimer`'s time) is kept by system identity across reload.
- Hot reload: a generation whose system keeps its identity, declaration and place keeps its Bevy
  system with no schedule edit. Adding, removing, moving or re-declaring a system costs one edit of
  each schedule it touches, at the frame boundary.
- `run_stage`'s loop and the four stage exclusive systems go. Startup, `OnEnter`/`OnExit`,
  observers and `call_entry` keep today's exclusive path, on the same fetch-and-call code.
- The main thread holds the isolate: every TS system takes `NonSendMarker` and the runtime. That
  param is the seam stage 2 replaces with a lane handle.
- Edit mode's skip (`stage_should_run`) becomes a run condition on `BevyTsSet` and the slot sets.
- `docs/research/script-threading.md` and its bench land with this change.
- No new dependency. No protocol change. No TS API change.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `script-runtime`: "JS runs on the main thread inside exclusive stage systems" becomes "JS runs
  on the main thread inside TS systems Bevy schedules"; Rust systems with no conflicting access
  may now run beside a TS system.
- `ts-systems`: "Schedules, ordering and run conditions": each TS system is a Bevy system in its
  stage's set, chained in run order; conditions are Bevy run conditions; deferred output applies
  at the sync point after the system.
- `native-ordering`: "Slots exist only for sets a generation names": a slot is a pair of system
  sets, not two exclusive systems; a reload that moves a system edits the schedules it touches.

## Impact

- Code: `crates/bevy-ts/src/systems.rs` (build, fetch, run, `run_stage` removed), a new
  `crates/bevy-ts/src/bevy_system.rs` (the `TsParams` system param, its builder, the deferred
  buffer), `sets.rs` (slots become sets), `generation.rs` (commit edits schedules), `lib.rs`
  (stages become sets), `memory.rs` (per-system handles), `host.rs` (cell and ticks from the
  system), `runtime.rs`, `observers.rs`.
- Docs: `docs/scripts.md`, `docs/native.md` (the "Parallel systems" row, slots), README's runtime
  sentence, `docs/research/script-threading.md` (lands here).
- Neighbours, both landed (writable-native-params 584b78cf, ecs-schedules stage 1 3d641f44); this
  change is rebased onto them:
  - ecs-schedules: its lazily-made stages become "add that schedule's systems at commit"; its
    collective `runIf` becomes Bevy's `run_if` on the group's set; its condition state moves to
    the condition systems, kept by identity across reload; `runSchedule`/`runSystem` apply from the
    deferred buffer. Agreed with lead@ecs-schedules 2026-10-23.
  - writable-native-params: the host's `UnsafeWorldCell` and run ticks come from the TS system's
    param; each exported param's kept access joins the system's declared access; TS output still
    applies before exported params' deferred state, in param order. Agreed with
    lead@writable-native-params 2026-10-23.
- Ask-first, the operator's go needed before the build:
  - Architecture: TS systems become Bevy systems built at run time; a custom `unsafe impl
    SystemParam` (`TsParams`) hands the bridge an `UnsafeWorldCell` limited to the access it
    declared.
  - Performance: a stage with N deferring TS systems gets up to N sync points instead of one
    barrier; a reload that changes a system's declaration rebuilds that schedule's graph. The build
    measures both before landing (task 6).
