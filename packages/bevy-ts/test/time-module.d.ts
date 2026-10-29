// What `bevy-ts types` writes for `bevy-ts/time` (types.rs checks it holds `emit_modules`'s
// output verbatim), for time_writes.ts.
declare module "bevy-ts/time" {
  /** `bevy_ts_embed::time::VirtualClock<'_>`, a system param exported from Rust; take it as a system parameter. */
  export interface VirtualTime {
    pause(): void;
    unpause(): void;
    toggle(): void;
    setRelativeSpeed(ratio: number): void;
    setRelativeSpeedF64(ratio: number): void;
    setMaxDelta(seconds: number): void;
    setWrapPeriod(seconds: number): void;
    readonly delta_secs: number;
    readonly elapsed_secs: number;
    readonly elapsed_secs_wrapped: number;
    readonly delta_secs_f64: number;
    readonly elapsed_secs_f64: number;
    readonly elapsed_secs_wrapped_f64: number;
    readonly wrap_period: number;
    readonly is_paused: boolean;
    readonly was_paused: boolean;
    readonly relative_speed: number;
    readonly relative_speed_f64: number;
    readonly effective_speed: number;
    readonly effective_speed_f64: number;
    readonly max_delta: number;
  }
  /** `bevy_ts_embed::time::FixedClock<'_>`, a system param exported from Rust; take it as a system parameter. */
  export interface FixedTime {
    setTimestep(seconds: number): void;
    setTimestepSeconds(seconds: number): void;
    setTimestepHz(hz: number): void;
    discardOverstep(seconds: number): void;
    setWrapPeriod(seconds: number): void;
    readonly delta_secs: number;
    readonly elapsed_secs: number;
    readonly elapsed_secs_wrapped: number;
    readonly delta_secs_f64: number;
    readonly elapsed_secs_f64: number;
    readonly elapsed_secs_wrapped_f64: number;
    readonly wrap_period: number;
    readonly timestep: number;
    readonly overstep: number;
    readonly overstep_fraction: number;
    readonly overstep_fraction_f64: number;
  }
}
