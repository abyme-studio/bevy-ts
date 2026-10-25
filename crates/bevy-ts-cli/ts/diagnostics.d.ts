/** Bevy's `Diagnostic`, as a declaration: a path (components joined by `/`) and how its history is kept. */
export declare class Diagnostic {
  constructor(path: string);
  readonly path: string;
  /** Shown after its value when logged (`"ms"`). Default none. */
  withSuffix(suffix: string): this;
  /** Measurements kept. Default 120. */
  withMaxHistoryLength(n: number): this;
  /** About how many seconds the smoothed value takes to follow a change. Default 2/21. */
  withSmoothingFactor(f: number): this;
}

/** Bevy's path constants, by plugin. List the plugin in bevy-ts.json's `diagnostics` to measure. */
export declare const FrameTimeDiagnosticsPlugin: { readonly FPS: "fps"; readonly FRAME_COUNT: "frame_count"; readonly FRAME_TIME: "frame_time" };
export declare const EntityCountDiagnosticsPlugin: { readonly ENTITY_COUNT: "entity_count" };
export declare const SystemInformationDiagnosticsPlugin: {
  readonly SYSTEM_CPU_USAGE: "system/cpu_usage";
  readonly SYSTEM_MEM_USAGE: "system/mem_usage";
  readonly PROCESS_CPU_USAGE: "process/cpu_usage";
  readonly PROCESS_MEM_USAGE: "process/mem_usage";
};
export declare const MeshAllocatorDiagnosticPlugin: {
  readonly SLABS: "mesh_allocator_slabs";
  readonly SLABS_SIZE: "mesh_allocator_slabs_size";
  readonly ALLOCATIONS: "mesh_allocator_allocations";
};
/** bevy-ts's own: each TS stage's CPU time in ms. */
export declare const ScriptStageDiagnosticsPlugin: { elapsedCpu(stage: string): string };

module "bevy-ts" {
  interface App {
    /** Bevy's `RegisterDiagnostic`: adds the path for this generation. A path a Rust plugin registered fails the load. */
    registerDiagnostic(d: import("bevy-ts/diagnostics").Diagnostic): App;
  }
}
