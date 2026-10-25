// `bevy-ts/diagnostics`: Bevy's diagnostics. The params (DiagnosticsStore, Diagnostics,
// FrameCount) are exported from Rust; this module adds Bevy's `Diagnostic` builder, the plugins'
// path constants and `app.registerDiagnostic`. Evaluated once, so each registration goes into the
// loading generation's `diagnostics` section.
import { app, sectionItems } from "bevy-ts";

/** Bevy's `Diagnostic`, as a declaration: a path and how its history is kept. */
export class Diagnostic {
  constructor(path) {
    this.path = String(path);
    this.suffix = "";
    this.maxHistoryLength = 120;
    this.smoothingFactor = 2 / 21;
  }
  withSuffix(suffix) {
    this.suffix = String(suffix);
    return this;
  }
  withMaxHistoryLength(n) {
    this.maxHistoryLength = n;
    return this;
  }
  withSmoothingFactor(f) {
    this.smoothingFactor = f;
    return this;
  }
}

const FRAME = /\(?((?:file|bevy-ts):[^\s()]+?):(\d+):(\d+)\)?\s*$/;
/** Where the script called from: the first frame outside the engine's own modules. */
function callerLocation() {
  for (const line of String(new Error().stack).split("\n").slice(1)) {
    if (/bevy-ts:/.test(line)) continue;
    const m = FRAME.exec(line);
    if (m) return { file: m[1].replace(/\?gen=\d+$/, ""), line: +m[2], column: +m[3] };
  }
  return undefined;
}

function located(message, location) {
  const e = new TypeError(message);
  e.bevyTsLocation = location;
  return e;
}

/** Bevy's `RegisterDiagnostic`: the path is the script's for this generation. */
app.registerDiagnostic = function registerDiagnostic(d) {
  const at = callerLocation();
  if (!(d instanceof Diagnostic)) throw located("registerDiagnostic takes a Diagnostic from bevy-ts/diagnostics", at);
  const name = `registerDiagnostic(${JSON.stringify(d.path)})`;
  if (!Number.isInteger(d.maxHistoryLength)) throw located(`${name}: withMaxHistoryLength takes a whole number`, at);
  if (typeof d.smoothingFactor !== "number" || !(d.smoothingFactor >= 0)) throw located(`${name}: withSmoothingFactor takes a number from 0`, at);
  sectionItems("diagnostics").push({ path: d.path, suffix: d.suffix, maxHistoryLength: d.maxHistoryLength, smoothingFactor: d.smoothingFactor, location: at || null });
  return app;
};

export const FrameTimeDiagnosticsPlugin = Object.freeze({ FPS: "fps", FRAME_COUNT: "frame_count", FRAME_TIME: "frame_time" });
export const EntityCountDiagnosticsPlugin = Object.freeze({ ENTITY_COUNT: "entity_count" });
export const SystemInformationDiagnosticsPlugin = Object.freeze({
  SYSTEM_CPU_USAGE: "system/cpu_usage",
  SYSTEM_MEM_USAGE: "system/mem_usage",
  PROCESS_CPU_USAGE: "process/cpu_usage",
  PROCESS_MEM_USAGE: "process/mem_usage",
});
export const MeshAllocatorDiagnosticPlugin = Object.freeze({
  SLABS: "mesh_allocator_slabs",
  SLABS_SIZE: "mesh_allocator_slabs_size",
  ALLOCATIONS: "mesh_allocator_allocations",
});
/** bevy-ts's own: each TS stage's CPU time, `bevy_ts/<stage in snake case>/elapsed_cpu`, in ms. */
export const ScriptStageDiagnosticsPlugin = Object.freeze({
  elapsedCpu: (stage) => `bevy_ts/${String(stage).replace(/(?<=[a-z])([A-Z])/g, "_$1").toLowerCase()}/elapsed_cpu`,
});
