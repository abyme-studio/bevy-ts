// Overwritten by run.mjs for each run.
export const C = { arm: "none" } as Config;
export interface Config {
  /** `none`, `keys`, `lines_hex`, `lines_srgb`, `lines_array` or `strip_f32`. */
  arm: string;
}
