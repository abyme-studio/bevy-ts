// `bevy-ts/testing`: load game modules outside the engine and call systems with hand-built params.
import type { QueryOf } from "bevy-ts";

export interface Declarations {
  readonly id: string;
  readonly kind: "system" | "setup";
  readonly params?: readonly { readonly kind: string }[];
  readonly error?: string;
}

/** Loads modules as a generation and returns the manifest the engine would receive. */
export declare function load(importer: () => Promise<unknown>): Promise<{
  systems: { name: string; schedule: { kind: string }; params: { kind: string }[] }[];
  components: { name: string }[];
  resources: { name: string }[];
}>;
/** The declarations the compile step attached to a system function. */
export declare function declarationsOf(fn: (...args: never[]) => unknown): Declarations | undefined;
/** A stand-in query over hand-built rows. */
export declare function rows<R extends unknown[]>(list: R[], entities?: number[]): QueryOf<R>;
