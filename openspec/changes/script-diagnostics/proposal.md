## Why

A TS system cannot read how the game is doing: no frame time, no entity count, no render timing,
not even a clock finer than `Date.now`. Heimdall's screen budget (heimdall/scale.md, gap 3) needs
counts and update milliseconds to move its cut, and the coverage matrix lists the whole category as
missing (`docs/research/bevy-coverage.md`, `DiagnosticsStore` and `FrameCount`, plan item 11).
Bevy has all of it in `bevy_diagnostic`; bevy-ts only has to reach it.

## What Changes

- A built-in module `bevy-ts/diagnostics`, in every engine, with three exported params named as
  Bevy names them:
  - `DiagnosticsStore`: reads every registered diagnostic by path (`value`, `average`,
    `smoothed`, `historyLen`, `maxHistoryLength`, `duration`, `isEnabled`, `suffix`, the history
    into a `Float64Array`), and lists the paths.
  - `Diagnostics`: Bevy's writer param. `addMeasurement(path, value)` goes through Bevy's own
    deferred buffer.
  - `FrameCount`: Bevy's `FrameCount`, `value()`.
- `app.registerDiagnostic(new Diagnostic(path).withSuffix("ms")...)`, Bevy's
  `RegisterDiagnostic`, so a script adds its own diagnostic. The module exports Bevy's path
  constants (`FrameTimeDiagnosticsPlugin.FPS`, `EntityCountDiagnosticsPlugin.ENTITY_COUNT`, ...).
- `bevy-ts.json` gains `diagnostics`, a list of Bevy diagnostics plugins the engine adds:
  `FrameTimeDiagnosticsPlugin`, `EntityCountDiagnosticsPlugin`,
  `SystemInformationDiagnosticsPlugin`, `LogDiagnosticsPlugin`, `RenderDiagnosticsPlugin`,
  `MeshAllocatorDiagnosticPlugin`, `RenderAssetDiagnosticPlugin<..>` for the engine's render
  assets, and `ScriptStageDiagnosticsPlugin` (below). Default: none. It needs no project engine,
  as `meshPicking` does not, and `bevy-ts build` keeps it in `game.json`.
- `ScriptStageDiagnosticsPlugin`, bevy-ts's own: each TS stage records its CPU time as a Bevy
  diagnostic (`bevy_ts/<stage>/elapsed_cpu`, ms), as Bevy's render diagnostics record a pass's.
- Exported methods take `&str` arguments and return `String` (UTF-8 across). Diagnostics are
  keyed by path, so this change needs it; every later export may use it.
- `performance.now()` in scripts: milliseconds since the engine started, from
  `bevy_platform`'s `Instant`.
- A coverage table of every Bevy 0.20 diagnostic source and what TS reaches, in
  `docs/diagnostics.md`; the matrix row moves to covered.

Not in scope, because Bevy lacks it: draw and batch counts. Bevy 0.20 counts no draws, and its
pipeline statistics run on Vulkan and DX12 only, not Metal. Visible-entity counts are not a Bevy
diagnostic either.

## Capabilities

### New Capabilities

- `script-diagnostics`: the `bevy-ts/diagnostics` module (`DiagnosticsStore`, `Diagnostics`,
  `FrameCount`, `Diagnostic`, path constants), `app.registerDiagnostic`, the `diagnostics` switch
  in `bevy-ts.json` and `game.json`, `ScriptStageDiagnosticsPlugin`, and the coverage table.

### Modified Capabilities

- `native-functions`: method signatures may take `&str` and return `String`.
- `script-runtime`: scripts have `performance.now()`.

## Impact

- Code: `crates/bevy-ts/src/host.rs` (a string kind: argument as a UTF-8 slice, result in a byte
  arena, JS glue encodes and decodes), `crates/bevy-ts-native-derive` (accept `&str`/`String`),
  a new `crates/bevy-ts-cli/src/diagnostics.rs` (the params, module JS and d.ts, the switch, the
  stage plugin), `crates/bevy-ts-cli/src/cli.rs` (`base_app` adds the listed plugins),
  `crates/bevy-ts-cli/src/build.rs` (`game.json`), `crates/bevy-ts/src/systems.rs` (stage
  timing), `crates/bevy-ts-v8/js/core.js` (`performance`), `docs/diagnostics.md`,
  `docs/native.md`, `docs/research/bevy-coverage.md`.
- Per-frame cost: none unless a project lists a plugin. Each listed plugin costs what it costs in
  Bevy (design.md, "Plugin costs"); a bench measures each arm before landing.
- Ask-first (operator's go needed):
  - Dependency: `SystemInformationDiagnosticsPlugin` needs Bevy's `sysinfo_plugin` feature, which
    compiles the `sysinfo` crate into every engine. Without the go, that one plugin is refused by
    name and the rest lands.
  - Performance: `EntityCountDiagnosticsPlugin` scans every entity slot each frame (Bevy's
    `Entities::count_spawned`); at a million entities that is a scan of a million each frame.
    `RenderDiagnosticsPlugin` adds a command encoder and readback per frame. Both are opt-in.
  - Pattern: strings across the exported-param boundary extend the host ABI. animation-control
    and others rejected designs for lack of it.
- Neighbours: script-time owns `Res<Time<..>>` and `Time<Real>`; this change reads no clock
  except `performance.now()`. writable-native-params (landed) gives deferred params and one
  `SystemState` per export.
