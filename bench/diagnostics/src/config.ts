// Overwritten by run.mjs for each run.
export const C = { n: 10000 } as Config;
export interface Config {
  /** Entities, each a `Transform` and a TS `Thing`; 1,000 of them also drawn as cubes. */
  n: number;
}
