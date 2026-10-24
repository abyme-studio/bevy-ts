## ADDED Requirements

### Requirement: The engine exports Bevy's diagnostics
Every engine (`bevy-ts dev`, `bevy-ts play`, a shipped game, `bevy-ts types`) SHALL register the built-in module `bevy-ts/diagnostics`, with three exported params named as Bevy names them, through the exported-param mechanism (native-functions):

- `DiagnosticsStore`, over Bevy's `Res<DiagnosticsStore>`. Each method takes a diagnostic path as a string and returns what Bevy's `Diagnostic` returns: `value(path)`, `average(path)`, `smoothed(path)`, `duration(path)` (seconds), `historyLen(path)`, `maxHistoryLength(path)`, `isEnabled(path)`, `suffix(path)`. Each SHALL return `undefined` for a path the store does not hold, and `value`, `average`, `smoothed` and `duration` SHALL also return `undefined` where Bevy returns `None`. `values(path, out: Float64Array): number` SHALL copy the history, oldest first, into `out` and return how many values it holds; it SHALL throw when `out` is shorter than the history. `len(): number` and `path(i): string | undefined` SHALL list the store's paths in sorted order.
- `Diagnostics`, over Bevy's `Diagnostics` param. `addMeasurement(path, value)` SHALL add a measurement through Bevy's deferred buffer, stamped when called, applied after the system's run. As in Bevy, a measurement for a path that is not registered, or not enabled, SHALL be dropped.
- `FrameCount`, over Bevy's `Res<FrameCount>`. `value(): number` SHALL return Bevy's frame count: 0 in the first update, wrapping at `u32::MAX`.

The module SHALL export Bevy's path constants for every plugin the `diagnostics` switch accepts, as Bevy names them (`FrameTimeDiagnosticsPlugin.FPS`, `.FRAME_TIME`, `.FRAME_COUNT`; `EntityCountDiagnosticsPlugin.ENTITY_COUNT`; `SystemInformationDiagnosticsPlugin.SYSTEM_CPU_USAGE`, `.SYSTEM_MEM_USAGE`, `.PROCESS_CPU_USAGE`, `.PROCESS_MEM_USAGE`), each a string holding Bevy's path. A path SHALL be Bevy's `DiagnosticPath` text: components joined by `/`.

#### Scenario: Frame time from a script
- **WHEN** a headless project lists `FrameTimeDiagnosticsPlugin` in `diagnostics`, and a TS system taking `store: DiagnosticsStore` reads `store.smoothed(FrameTimeDiagnosticsPlugin.FRAME_TIME)` after 30 frames
- **THEN** it reads a number greater than 0, in milliseconds, equal to what a Rust system reads from the same store that frame

#### Scenario: A path nobody registered
- **WHEN** a system reads `store.value("nope")`
- **THEN** it reads `undefined`, and the call does not throw

#### Scenario: The frame count
- **WHEN** a `Startup` TS system reads `frames.value()` and an `Update` system reads it on the third update
- **THEN** the startup system reads 0 and the update system reads 2

#### Scenario: Listing the paths
- **WHEN** `FrameTimeDiagnosticsPlugin` and `EntityCountDiagnosticsPlugin` are listed and a system walks `store.path(i)` for `i` below `store.len()`
- **THEN** it reads `entity_count`, `fps`, `frame_count`, `frame_time`, in that order

### Requirement: Scripts register their own diagnostics
`bevy-ts/diagnostics` SHALL export `Diagnostic`, Bevy's builder: `new Diagnostic(path)`, `withSuffix(s)`, `withMaxHistoryLength(n)` and `withSmoothingFactor(f)`, with Bevy's defaults (history 120, smoothing 2/21, no suffix). `app.registerDiagnostic(d)`, Bevy's `RegisterDiagnostic`, SHALL add `d` to Bevy's `DiagnosticsStore` when the generation loads. A diagnostic SHALL be declared by every generation, as messages are:

- the same path with the same settings on reload SHALL keep its history;
- the same path with other settings SHALL replace it, with an empty history;
- a path the next generation no longer declares SHALL stay in the store, disabled, so it records nothing;
- a path a Rust plugin registered SHALL NOT be replaced by a script; registering it SHALL fail the load, naming the path.

#### Scenario: A script's own measurement
- **WHEN** a plugin calls `app.registerDiagnostic(new Diagnostic("game/spawned").withSuffix("rows"))`, and an `Update` system calls `diagnostics.addMeasurement("game/spawned", 40)`
- **THEN** on the next update `store.value("game/spawned")` reads 40 and `store.suffix("game/spawned")` reads `"rows"`, and a Rust system reads the same from `DiagnosticsStore`

#### Scenario: A reload keeps the history
- **WHEN** the file holding that system is saved with a change elsewhere and the generation reloads
- **THEN** `store.historyLen("game/spawned")` is not reset

#### Scenario: Measuring before registering
- **WHEN** a system calls `diagnostics.addMeasurement("game/unregistered", 1)`
- **THEN** nothing is recorded, and `store.value("game/unregistered")` reads `undefined`

### Requirement: A project lists the diagnostics plugins it wants
`bevy-ts.json` SHALL accept `diagnostics`, a list of Bevy diagnostics plugin names, default empty. `bevy-ts dev` SHALL add each listed plugin with its default settings, under a window, `--viewport` and `--headless` alike, unless the app already has it. The accepted names SHALL be `FrameTimeDiagnosticsPlugin`, `EntityCountDiagnosticsPlugin`, `SystemInformationDiagnosticsPlugin`, `LogDiagnosticsPlugin`, `RenderDiagnosticsPlugin`, `MeshAllocatorDiagnosticPlugin`, `RenderAssetDiagnosticPlugin<T>` for each render asset the engine prepares, named by Bevy's type name, and `ScriptStageDiagnosticsPlugin`. A plugin that needs a render app SHALL be added only when the engine renders; under `bevy-ts types` it SHALL be skipped. The switch SHALL NOT need a project engine: it is not an engine hash input and starts no `cargo`. A name not on the list SHALL fail `bevy-ts dev`, naming the name, `diagnostics` and the accepted names. `DiagnosticsPlugin` and `FrameCountPlugin` SHALL NOT be listed: every engine has them through Bevy's default plugins.

`bevy-ts build` SHALL write the project's `diagnostics` into `game/game.json`, and playing SHALL add the plugins it lists. A `game.json` without `diagnostics` SHALL play with none.

#### Scenario: Off by default
- **WHEN** a project's `bevy-ts.json` has no `diagnostics`
- **THEN** the app's `DiagnosticsStore` holds no path, and no diagnostics system runs in `Update`

#### Scenario: A script-only game reads its frame time
- **WHEN** a project with `"diagnostics": ["FrameTimeDiagnosticsPlugin"]` and no `native` runs `bevy-ts dev --headless`
- **THEN** the prebuilt engine runs, and `store.value("fps")` reads a number after the second update

#### Scenario: A misspelt plugin
- **WHEN** `bevy-ts.json` holds `"diagnostics": ["FrameTimeDiagnostic"]`
- **THEN** `bevy-ts dev` exits 1 with a message naming `FrameTimeDiagnostic`, `diagnostics` and `FrameTimeDiagnosticsPlugin`

#### Scenario: Built and played
- **WHEN** a project listing `EntityCountDiagnosticsPlugin` is built and its folder played
- **THEN** `game.json` lists it, and the played app's store holds `entity_count`

### Requirement: Script stages record their CPU time
`ScriptStageDiagnosticsPlugin`, when listed, SHALL register one Bevy diagnostic per TS stage that runs, at `bevy_ts/<stage>/elapsed_cpu` with the stage in kebab case (`bevy_ts/update/elapsed_cpu`) and suffix `ms`, and SHALL add one measurement each time the stage's exclusive system runs: the wall time of the whole stage, JS and host work together. A stage with no TS systems SHALL record nothing. Unlisted, no stage SHALL read a clock for it.

#### Scenario: Update's cost
- **WHEN** a headless project lists `ScriptStageDiagnosticsPlugin` and has an `Update` TS system that busy-loops for 2 ms by `performance.now()`
- **THEN** after 10 updates `store.average("bevy_ts/update/elapsed_cpu")` reads at least 2

### Requirement: Diagnostics coverage
`docs/diagnostics.md` SHALL hold a coverage table with one row for every diagnostic source in Bevy 0.20 (`bevy_diagnostic`'s plugins, `bevy_render`'s diagnostic plugins, `FrameCount`), each with what TS reaches and the test that shows it, and a row for each thing Bevy does not have that a reader may look for (draw counts, batch counts, visible entities), saying Bevy lacks it. Each covered row SHALL name a test.

#### Scenario: A reader looks for draw calls
- **WHEN** a reader looks up draw counts in `docs/diagnostics.md`
- **THEN** the table says Bevy 0.20 counts no draws, and pipeline statistics run on Vulkan and DX12 only
