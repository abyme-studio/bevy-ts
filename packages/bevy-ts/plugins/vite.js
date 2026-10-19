// `bevy-ts/vite`: a Vite (and vitest) plugin that compiles TS through bevy-ts, so systems carry
// their declarations outside the engine.
import { TS_FILE, transform } from "./transform.js";

export default function bevyTs() {
  return {
    name: "bevy-ts",
    enforce: "pre",
    transform(code, id) {
      const file = id.split("?")[0];
      if (!TS_FILE.test(file) || file.includes("/node_modules/")) return null;
      return { code: transform(file), map: null };
    },
  };
}
