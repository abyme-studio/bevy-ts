## Context

Bevy keeps every diagnostic in one resource, `DiagnosticsStore`: a map from `DiagnosticPath` (a
`/`-joined string) to a `Diagnostic` (a ring of timed measurements, a sum, an EMA, a suffix, an
enabled flag). Plugins register paths at build and add a system that measures each frame through
the `Diagnostics` param, whose `Deferred` buffer applies after the system. Render diagnostics
arrive late: `sync_diagnostics` adds their paths to the store in `PreUpdate` as passes report them.

bevy-ts reaches none of it. `DefaultPlugins` already adds `DiagnosticsPlugin` (the store) and
`FrameCountPlugin`, so the store exists and is empty. `FrameCount` has no `Reflect`, so the generic
`Res<T>` path cannot read it. Exported params take no strings (animation-control's design notes
this twice), and diagnostics are keyed by string.

writable-native-params landed: an export may hold `Deferred<T>` params, every export has one
`SystemState` shared by all TS systems, and calls run at the calling system's ticks.

## Goals / Non-Goals

**Goals:**
- Every Bevy 0.20 diagnostic source readable from TS, by Bevy's names and paths.
- Scripts add their own diagnostics and measurements, Bevy's way.
- A project turns on Bevy's diagnostics plugins without Rust.
- Update milliseconds: a script clock and the TS stages' own CPU time.
- No cost to a project that turns nothing on.

**Non-Goals:**
- Draw, batch and visible-entity counts: Bevy 0.20 has none.
- A diagnostics overlay or UI. Bevy has none built in (it has `LogDiagnosticsPlugin`).
- `Time<Real>` and the other clocks: script-time's.
- Plugin settings (history length, filters for `LogDiagnosticsPlugin`): defaults only for now.

## Decisions

1. **Three exports with Bevy's names, in `bevy-ts/diagnostics`.** `DiagnosticsStore` reads,
   `Diagnostics` writes, `FrameCount` reads. A TS system writes `store: DiagnosticsStore` where a
   Rust one writes `store: Res<DiagnosticsStore>`. *Alternative:* `stands_for =
   "Res<DiagnosticsStore>"`. Rejected: neither type is reflected, so TS does not declare the inner
   type and the claim would not be typed (docs/native.md, "Standing for a Bevy spelling").
   Registered by every engine in `crates/bevy-ts-cli/src/diagnostics.rs`, as `Cameras` is.

2. **`Diagnostics` wraps Bevy's own param.** The export is
   `struct Diagnostics<'w, 's> { inner: bevy::diagnostic::Diagnostics<'w, 's> }`, and
   `add_measurement(&mut self, path: &str, value: f64)` calls `inner.add_measurement(&path, ||
   value)`. Bevy's private `DiagnosticsBuffer` applies after the run, through
   writable-native-params' deferred apply. Same timing, same drop rule (unregistered or disabled is
   dropped) as a Rust system. *Alternative:* `ResMut<DiagnosticsStore>` and write in place.
   Rejected: a measurement would land mid-run, unlike Bevy, and the store would be exclusive to
   every TS system holding the export.

3. **Strings cross as UTF-8 slices.** A `&str` argument is a new `Kind::Str` the derive accepts.
   The glue encodes it with `encodeInto` into a scratch `Uint8Array` grown as needed, and passes
   it as one of the call's four slices; Rust checks UTF-8 and borrows it. A `String` result is
   written into a per-call byte arena beside `H.result`, its length in a slot, and decoded by
   the glue's `TextDecoder`. Strings are not slice elements or record fields, so records keep a
   fixed layout. This is general: every export may use it. *Alternative:* the store's paths as
   numbers. Rejected: render paths appear at run time and `HashMap` order is not stable, so ids
   need a second table and a lookup by name anyway.

4. **Paths listed sorted.** `len()` and `path(i)` read the store's keys sorted. A store holds
   tens of paths; sorting on each call is cheap, and a system that walks them gets a stable order.
   Bevy's `iter()` order is the map's, which changes as paths are added.

5. **`registerDiagnostic` is a per-generation declaration.** The module adds
   `app.registerDiagnostic`, as Bevy's `RegisterDiagnostic` extends `App`, and declares the
   module's `App` augmentation in its d.ts. It writes a manifest section `diagnostics`, read on
   load by the diagnostics plugin. bevy-ts keeps the set of paths scripts own in a resource:
   - a new path is `DiagnosticsStore::add`ed;
   - an owned path with equal settings is left alone (history kept);
   - an owned path with other settings is replaced;
   - an owned path no longer declared gets `is_enabled = false`;
   - a path in the store that scripts do not own is Rust's, and declaring it fails the load.
   *Alternative:* a deferred `register` method on `Diagnostics`. Rejected: Bevy registers at
   build, not from systems, and a measurement in the same run would be dropped.

6. **The `diagnostics` switch follows `meshPicking`.** A list of Bevy plugin names in
   `bevy-ts.json` and `game.json`, read in `base_app`, each added with `Default` unless already
   added. It is not an engine hash input. Bevy's names, not bevy-ts aliases, so a reader can look
   each one up in Bevy's docs. `RenderAssetDiagnosticPlugin<T>` takes the render asset types the
   engine prepares (`RenderMesh`, `GpuImage` and the others `bevy-ts types` finds), written as
   Bevy prints the type (`RenderAssetDiagnosticPlugin<RenderMesh>`), from a fixed table in
   `diagnostics.rs`.

7. **`ScriptStageDiagnosticsPlugin` is bevy-ts's own, Bevy-shaped.** Bevy times each render pass
   as `render/<pass>/elapsed_cpu`. bevy-ts times each TS stage the same way:
   `bevy_ts/<stage>/elapsed_cpu`, in `run_stage`, two `Instant::now` reads per stage run, only
   when the plugin is listed. It registers a path the first time a stage runs, in its own
   `DiagnosticsStore::add`. This gives the budget its update milliseconds without each script
   timing itself. *Alternative:* scripts time themselves with `performance.now()`. Kept as well,
   but it misses bevy-ts's own work in the stage (column views, commands).

8. **`performance.now()` from `bevy_platform::time::Instant`.** One host function returning
   `f64` milliseconds since the runtime's `Instant` was taken; `timeOrigin` from `SystemTime` at
   the same moment. Defined in `core.js` beside `console`. Not a Bevy feature, a web one every TS
   author expects; `Date.now` stays.

## Plugin costs

Read from Bevy 0.20 source, not measured. Every one is off unless listed.

| Plugin | Per frame | Where |
| --- | --- | --- |
| `DiagnosticsPlugin`, `FrameCountPlugin` | already in every engine; `FrameCount` is one `Last` system adding 1 | main |
| `FrameTimeDiagnosticsPlugin` | one `Update` system, three measurements into a ring of 120 | main |
| `EntityCountDiagnosticsPlugin` | one `Update` system; `Entities::count_spawned` walks every entity slot, so O(entities) | main |
| `SystemInformationDiagnosticsPlugin` | a task refreshing process, CPU and memory every 200 ms (sysinfo's minimum on macOS and Linux), plus two small systems; needs the `sysinfo_plugin` feature | compute pool, main |
| `LogDiagnosticsPlugin` | one system; logs every second | main |
| `RenderDiagnosticsPlugin` | spans around each pass, one extra command encoder and a readback per frame; on Metal CPU time only (Bevy: timestamps and pipeline statistics need Vulkan or DX12) | render |
| `MeshAllocatorDiagnosticPlugin`, `RenderAssetDiagnosticPlugin<T>` | one extract system and one `PreUpdate` system each | render, main |
| `ScriptStageDiagnosticsPlugin` | two clock reads and one measurement per TS stage run | main |

The bench (`bench/diagnostics`) measures each arm at 10k and 100k entities, 3 runs per arm, frame
and main-world time, before the change lands.

## Risks / Trade-offs

- **Entity count at scale.** At a million entities `count_spawned` scans a million slots each
  frame. That is Bevy's cost; the docs say so and suggest counting what matters with a query.
- **`sysinfo` in the engine.** Turning on `sysinfo_plugin` compiles a crate into every engine,
  used or not. Ask first; without a go, `SystemInformationDiagnosticsPlugin` is refused by name
  ("needs the sysinfo_plugin feature, which this engine was built without") and the rest lands.
- **Render diagnostics on Metal** give CPU time per pass only. The docs say so.
- **The string path costs a copy** each call (encode in, decode out). Diagnostics read a few
  values a frame; a hot path keeps numbers.
- **Headless pacing.** Frame time under `--headless` is paced near 21 ms whatever runs
  (heimdall/scale.md); the docs point budgets at counts and `elapsed_cpu`, not frame time.

## Migration Plan

Additive. Stages land alone:
1. Strings across exports (native-functions), with its tests.
2. `performance.now()`.
3. `bevy-ts/diagnostics` params, `registerDiagnostic`, the switch, `game.json`.
4. `ScriptStageDiagnosticsPlugin`, the bench, `docs/diagnostics.md`, the coverage row.

## Open Questions

- Whether the `sysinfo_plugin` feature gets the operator's go (Impact, ask-first).
