// Overwritten by run.mjs for each run.
export const C = { n: 1000, arm: "grow" } as Config;
export interface Config {
  /** Rows. */
  n: number;
  /**
   * "grow": a keyed <For> from 0 to n in one set; "grow1k": 1,000 more a frame; "reverse": n rows
   * mounted, then reversed in one set; "clear": n rows mounted, then emptied in one set; "spawn": the
   * same n rows with `commands.spawn`, one frame, the reference.
   */
  arm: "grow" | "grow1k" | "reverse" | "clear" | "spawn";
}
