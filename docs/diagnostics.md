# Diagnostics

TS reads and writes Bevy's diagnostics under Bevy's names, from `bevy-ts/diagnostics`. Nothing
measures until `bevy-ts.json` lists a plugin.

```json
{ "diagnostics": ["FrameTimeDiagnosticsPlugin", "EntityCountDiagnosticsPlugin", "ScriptStageDiagnosticsPlugin"] }
```

```ts
import { app, Update } from "bevy-ts";
import type { Diagnostics, DiagnosticsStore, FrameCount } from "bevy-ts/diagnostics";
import { Diagnostic, EntityCountDiagnosticsPlugin, ScriptStageDiagnosticsPlugin } from "bevy-ts/diagnostics";

app.registerDiagnostic(new Diagnostic("game/streamed").withSuffix(" rows"));

// The cut moves when the update stage costs more than 4 ms on average.
export function budget(store: DiagnosticsStore, d: Diagnostics, frames: FrameCount) {
  const entities = store.value(EntityCountDiagnosticsPlugin.ENTITY_COUNT) ?? 0;
  const update = store.average(ScriptStageDiagnosticsPlugin.elapsedCpu("Update")) ?? 0;
  if (frames.value() % 60 === 0) console.log(`${entities} entities, update ${update.toFixed(2)} ms`);
  d.addMeasurement("game/streamed", entities);
}
app.addSystems(Update, budget);
```

## The params

| Param | Bevy | Reads or writes |
|---|---|---|
| `DiagnosticsStore` | `Res<DiagnosticsStore>` | by path: `value`, `average`, `smoothed`, `historyLen`, `maxHistoryLength`, `duration` (seconds), `isEnabled`, `suffix`, `history(path, out: Float64Array)`; `len()` and `path(i)` list the paths sorted. An unknown path reads `undefined` |
| `Diagnostics` | `Diagnostics` | `addMeasurement(path, value)`, through Bevy's own buffer: it lands after the system runs, and a path no one registered, or one disabled, is dropped |
| `FrameCount` | `Res<FrameCount>` | `value()`: frames since the start, 0 in `Startup`, updated in `Last` |

`app.registerDiagnostic(new Diagnostic(path).withSuffix(s).withMaxHistoryLength(n).withSmoothingFactor(f))`
is Bevy's `RegisterDiagnostic`. A path is the script's for the generation: a reload with the same
settings keeps its history, other settings replace it, and a path no longer registered is disabled.
A path a Rust plugin registered fails the load at the call.

## The switch

`diagnostics` in `bevy-ts.json` lists Bevy plugins by Bevy's names; `bevy-ts build` keeps the
list in `game.json`, so a played game measures what `bevy-ts dev` did. It needs no project engine.
A name not below fails the start, naming the accepted ones. Render plugins are added only where
the engine renders.

| Plugin | Paths | Per frame |
|---|---|---|
| `FrameTimeDiagnosticsPlugin` | `fps`, `frame_time` (ms), `frame_count` | one `Update` system |
| `EntityCountDiagnosticsPlugin` | `entity_count` | one `Update` system that walks every entity slot: O(entities) |
| `SystemInformationDiagnosticsPlugin` | `system/cpu_usage`, `system/mem_usage`, `process/cpu_usage`, `process/mem_usage` | a refresh every 200 ms on the compute pool; nothing under Bevy's `dynamic_linking` |
| `LogDiagnosticsPlugin` | none: logs every diagnostic each second | one system |
| `RenderDiagnosticsPlugin` | `render/<pass>/elapsed_cpu` (and `elapsed_gpu` where the GPU allows) | a span per pass and a readback; on Metal CPU time only |
| `MeshAllocatorDiagnosticPlugin` | `mesh_allocator_slabs`, `mesh_allocator_slabs_size`, `mesh_allocator_allocations` | one extract and one `PreUpdate` system |
| `RenderAssetDiagnosticPlugin<RenderMesh>`, `<GpuImage>`, `<GpuShaderBuffer>` | `render_asset/<type>` | one extract and one `PreUpdate` system |
| `ScriptStageDiagnosticsPlugin` (bevy-ts's) | `bevy_ts/<stage>/elapsed_cpu` (ms) | two clock reads per TS stage that runs systems |

`ScriptStageDiagnosticsPlugin` times each TS stage as Bevy times a render pass: its systems and
bevy-ts's own work in the stage (column views, commands). `performance.now()` times anything else:
milliseconds since the engine started, finer than `Date.now`, not reset by a reload.

Measured (`bench/diagnostics`, a release engine headless, 8 runs, medians): no plugin adds main-world
time above the noise, about 1.5 ms on a loaded Mac, at 10k or 100k entities.
`EntityCountDiagnosticsPlugin` reads 0.1 ms or less; `RenderDiagnosticsPlugin` about 0.5 ms. The
table per plugin is in the change's design (`openspec/changes/archive/*-script-diagnostics/design.md`).

## Headless pacing

Under `--headless` the frame is paced near 21 ms whatever runs, so `frame_time` and `fps` say
little there. A budget reads counts and `elapsed_cpu`.

## Coverage

Every diagnostic source Bevy 0.20 has, and what TS reaches.

| Source | TS | Test |
|---|---|---|
| `DiagnosticsStore`, every field of a `Diagnostic` | `DiagnosticsStore` | `crates/bevy-ts-cli/tests/it/diagnostics.rs` |
| `Diagnostics` (writer) | `Diagnostics` | same |
| `RegisterDiagnostic` | `app.registerDiagnostic` | same |
| `FrameCount` | `FrameCount` | same |
| `FrameTimeDiagnosticsPlugin`, `EntityCountDiagnosticsPlugin`, `LogDiagnosticsPlugin` | the switch | same |
| `SystemInformationDiagnosticsPlugin` | the switch; Bevy compiles it to nothing under `dynamic_linking`, so only a release engine measures | `bench/diagnostics` |
| `RenderDiagnosticsPlugin`, `MeshAllocatorDiagnosticPlugin`, `RenderAssetDiagnosticPlugin<T>` | the switch, when rendering | `bench/diagnostics` |
| `AssetPlugin`'s `started_load_count` | `DiagnosticsStore`: Bevy registers it in every app, listed or not | `diagnostics.rs` |
| TS stage CPU time | `ScriptStageDiagnosticsPlugin` | `diagnostics.rs` |
| Draw and batch counts | Bevy lacks it | — |
| Pipeline statistics | Bevy has them on Vulkan and DX12 only, not Metal | — |
| Visible-entity counts | Bevy lacks it as a diagnostic: count `ViewVisibility` with a query | — |
